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
  // The map picker opens by itself for a fresh host; the lobby is behind it until Done.
  await host.getByTestId("map-picker-done").click();
  const hostUrl = host.url();
  expect(hostUrl).not.toContain("#");
  await expect(host.getByTestId("start-game-button")).toBeDisabled();
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
  await expect(guest.getByRole("button", { name: "Leave lobby" })).toBeVisible();
  await expect(guest.getByText(/Position 2: Player 2/)).toBeVisible();
  await guest.getByRole("button", { name: "Leave lobby" }).click();
  await expect(guest).toHaveURL(new URL("/", gameUrl).href);
  await expect(host.getByText(/Position 2: Open/)).toBeVisible();
  await guest.goto(gameUrl);
  await guest.getByRole("button", { name: "Join game" }).click();
  await expect(guest.getByRole("button", { name: "Leave lobby" })).toBeVisible();
  // Nobody says Ready: the host starts when every seat is filled.
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

test("watches a running game, and revokes a disconnected player on takeover", async ({
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
  // The map picker opens by itself for a fresh host; the lobby is behind it until Done.
  await host.getByTestId("map-picker-done").click();
  const gameUrl = host.url();
  const gameId = new URL(gameUrl).pathname.split("/").pop()!;
  const hostSession = await host.evaluate(
    (id) => sessionStorage.getItem(`ti4.player-session:${id}`),
    gameId,
  );
  expect(hostSession).toBeTruthy();
  // No host moves a seat: the seats are drawn when the game starts.
  await expect(host.getByRole("button", { name: /Move position/ })).toHaveCount(0);
  await expect(host.getByText(/Position 1: Player 1 \(Host\)/)).toBeVisible();
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  failOnBrowserErrors(guest);
  await guest.goto(gameUrl);
  await guest.getByLabel("Nickname").fill("Player 2");
  await guest.getByRole("button", { name: "Join game" }).click();
  await expect(guest.getByText(/Position 2: Player 2/)).toBeVisible();
  const guestSession = await guest.evaluate(
    (id) => sessionStorage.getItem(`ti4.player-session:${id}`),
    gameId,
  );
  expect(guestSession).toBeTruthy();
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
        // The seats were drawn at Start: a player is found by its name, not by its place.
        const named = (nickname: string) =>
          lobby.slots.find((slot: { nickname: string }) => slot.nickname === nickname);
        expect(named("Player 1").connected).toBe(true);
        return named("Player 2").can_take_over;
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
    (await request.get(`${base}/lobby`).then((r) => r.json())).slots.find(
      (slot: { nickname: string }) => slot.nickname === "Player 2",
    ).occupant,
  );
  await takeoverContext.close();
  await spectatorContext.close();
  await hostContext.close();
});

test("the host picks a map, a guest sees it within seconds, and the started board matches the preview", async ({
  browser,
  request,
}) => {
  const backend = `http://127.0.0.1:${process.env.TI4_E2E_BACKEND_PORT ?? "8080"}`;
  const hostContext = await browser.newContext();
  const host = await hostContext.newPage();
  failOnBrowserErrors(host);
  await host.goto("/");
  await host.getByLabel("Players").selectOption("3");
  await host.getByLabel("Nickname").fill("Mapper");
  await host.getByTestId("create-game-button").click();

  // Opens by itself, offers only maps the server says build, plus Random.
  const picker = host.getByRole("dialog", { name: "Choose your map" });
  await expect(picker).toBeVisible();
  const listed: { alias: string; buildable: boolean }[] = await (
    await request.get(`${backend}/api/maps?player_count=3`)
  ).json();
  expect(listed.length).toBeGreaterThan(0);
  await expect(host.getByTestId("map-picker-loading")).toBeHidden();
  await expect(picker.locator('[data-testid^="map-card-"]')).toHaveCount(listed.length + 1);
  await expect(host.getByTestId("map-preview-board")).toBeVisible();

  // A guest joins from the shared URL and watches the lobby.
  const gameUrl = host.url();
  const guestContext = await browser.newContext();
  const guest = await guestContext.newPage();
  failOnBrowserErrors(guest);
  await guest.goto(gameUrl);
  await guest.getByLabel("Nickname").fill("Guest");
  await guest.getByRole("button", { name: "Join game" }).click();
  await expect(guest.getByTestId("lobby-map-button")).toHaveText("View map");
  // A third player fills the last seat over the API.
  const gameIdForThird = new URL(gameUrl).pathname.split("/").pop()!;
  const third = await (
    await request.post(`${backend}/api/games/${gameIdForThird}/lobby/join`, {
      data: { kind: "new", nickname: "Third" },
    })
  ).json();

  const gameId = new URL(gameUrl).pathname.split("/").pop()!;

  // The host picks another template; the guest's row follows (polled every 2 s).
  const other = listed.find((m) => !(m as { recommended?: boolean }).recommended) ?? listed[0];
  await host.getByTestId(`map-card-${other.alias}`).click();
  await expect(host.getByTestId(`map-card-${other.alias}`)).toHaveAttribute("aria-pressed", "true");
  await expect(guest.getByTestId("lobby-map-row")).toContainText(other.alias, { timeout: 6_000 });
  await expect(guest.getByTestId("map-change-notice")).toContainText("The host changed the map");

  // Re-roll Random until the picture is settled, then keep it.
  await host.getByTestId("map-card-random").click();
  await expect(host.getByTestId("map-card-random")).toHaveAttribute("aria-pressed", "true");
  await expect(host.getByTestId("map-picker-summary")).toContainText("Random map");
  await host.getByTestId("map-reroll").click();
  await expect(host.getByTestId("map-reroll")).toHaveText("Re-roll");
  const previewIds = async () =>
    host
      .getByTestId("map-preview-board")
      .locator("[data-testid^='map-tile-']")
      .evaluateAll((nodes) =>
        nodes.map((n) => n.getAttribute("data-testid")!.replace("map-tile-", "")).sort(),
      );
  const serverPreviewIds = async (): Promise<string[]> => {
    const preview: { tiles: { system_id: string; special_area?: string }[] } = await (
      await request.get(`${backend}/api/games/${gameId}/lobby/map-preview`)
    ).json();
    return preview.tiles
      .filter((tile) => !tile.special_area)
      .map((tile) => tile.system_id)
      .sort();
  };
  await expect.poll(previewIds).toEqual(await serverPreviewIds());
  const before = await serverPreviewIds();
  await host.getByTestId("map-picker-done").click();

  // Guest opens the read-only view: no host controls, and no seat is marked as anybody's.
  await guest.getByTestId("lobby-map-button").click();
  const readonly = guest.getByRole("dialog", { name: "Map" });
  await expect(readonly.getByTestId("map-reroll")).toHaveCount(0);
  await expect(readonly.locator('[data-mine="true"]')).toHaveCount(0);
  await readonly.getByTestId("map-picker-close").click();

  // Start: the real board has the previewed systems.
  await expect(host.getByTestId("start-game-button")).toBeEnabled();
  await host.getByTestId("start-game-button").click();
  await expect(host.getByTestId("turn-status-bar")).toBeVisible();
  const board: { system_id: string; special_area?: string }[] = await (
    await request.get(`${backend}/api/games/${gameId}/map`)
  ).json();
  const actual = board
    .filter((tile) => !tile.special_area)
    .map((tile) => tile.system_id)
    .sort();
  // The preview is the board of the game but for its homes: a home of the preview is the place of
  // a seat (`seat_1`, …), and the game puts the home of the faction that was drawn there.
  const places = before.filter((id) => id.startsWith("seat_"));
  const systems = before.filter((id) => !id.startsWith("seat_"));
  expect(places).toHaveLength(3);
  expect(actual).toEqual(expect.arrayContaining(systems));
  expect(actual.length).toBeLessThanOrEqual(before.length);
  await hostContext.close();
  await guestContext.close();
});
