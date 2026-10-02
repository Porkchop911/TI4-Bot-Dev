import { test, expect, type Page } from "@playwright/test";
import { createStartedGame, gameSnapshot, openPlayerGame } from "./lobbyHelpers";

/**
 * Asserts core UI and system invariants on a given player or spectator page.
 */
async function assertPageInvariants(
  page: Page,
  expectedPrivatePosition?: number,
  isSpectator = false,
) {
  // 1. Connection must be connected
  const indicator = page.locator('[data-testid="connection-indicator"]');
  await expect(indicator).toHaveAttribute("data-status", "connected");

  // 2. SVG Board must be rendered and responsive
  const boardSvg = page.locator('[data-testid="ti4-board-svg"]');
  await expect(boardSvg).toBeVisible();

  // 3. Privacy Invariant: Spectators & opponents must never have private cards in DOM
  if (isSpectator) {
    const privateCards = page.locator('[data-private-card="true"]');
    await expect(privateCards).toHaveCount(0);
  } else if (expectedPrivatePosition) {
    // The presentation labels private cards by lobby position, not raw player ID.
    const foreignCards = page.locator(
      `[data-private-card="true"]:not([data-private-card-owner="position-${expectedPrivatePosition}"])`,
    );
    await expect(foreignCards).toHaveCount(0);
  }

  // 4. Arithmetic & State Invariant: Player resources must be valid non-negative integers
  const vpBadges = page.getByTestId("player-vp");
  await expect(vpBadges).toHaveCount(3);
  const count = await vpBadges.count();
  expect(count).toBeGreaterThan(0);
  for (let i = 0; i < count; i++) {
    const text = await vpBadges.nth(i).innerText();
    const match = /(\d+)\s*VP/.exec(text);
    expect(match).not.toBeNull();
    const vp = parseInt(match![1], 10);
    expect(vp).toBeGreaterThanOrEqual(0);
  }
}

test.describe("Multiplayer Online Flow & Invariant Suite", () => {
  test("three-seat strategy draft and first Action decision advance on the server", async ({
    browser,
    request,
  }) => {
    // Setup listeners for zero console errors / unhandled exceptions
    const trackErrors = (page: Page, label: string) => {
      page.on("pageerror", (err) => {
        throw new Error(`[${label}] Unhandled browser error: ${err.message}`);
      });
      page.on("console", (msg) => {
        if (msg.type() === "error") {
          // Ignore harmless favicon 404s
          if (!msg.text().includes("favicon")) {
            throw new Error(`[${label}] Console Error: ${msg.text()}`);
          }
        }
      });
    };

    // Create a fresh isolated game for this test run
    const { gameId, players } = await createStartedGame(request, 3, 42);
    const [p1, p2, p3] = players;

    // 1. Open Player 1 (p1)
    const contextP1 = await browser.newContext();
    const pageP1 = await contextP1.newPage();
    trackErrors(pageP1, "Player 1");
    await openPlayerGame(pageP1, gameId, p1.session);

    // 2. Open Player 2 (p2)
    const contextP2 = await browser.newContext();
    const pageP2 = await contextP2.newPage();
    trackErrors(pageP2, "Player 2");
    await openPlayerGame(pageP2, gameId, p2.session);

    const contextP3 = await browser.newContext();
    const pageP3 = await contextP3.newPage();
    trackErrors(pageP3, "Player 3");
    await openPlayerGame(pageP3, gameId, p3.session);
    const pages = [pageP1, pageP2, pageP3];

    // Wait for all claimed seats to connect.
    await expect(pageP1.locator('[data-testid="turn-status-bar"]')).toBeVisible();
    await expect(pageP2.locator('[data-testid="turn-status-bar"]')).toBeVisible();
    await expect(pageP3.locator('[data-testid="turn-status-bar"]')).toBeVisible();

    // Initial invariant checks across all claimed tabs.
    await assertPageInvariants(pageP1, 1);
    await assertPageInvariants(pageP2, 2);
    await assertPageInvariants(pageP3, 3);

    // Active Seat Choice Invariant:
    // P1 must have pending-choice-dialog; P2 must not.
    const p1Modal = pageP1.locator('[data-testid="pending-choice-dialog"]');
    await expect(p1Modal).toBeVisible();

    const p2Modal = pageP2.locator('[data-testid="pending-choice-dialog"]');
    await expect(p2Modal).toHaveCount(0);
    await expect(pageP3.getByTestId("pending-choice-dialog")).toHaveCount(0);
    const firstDraft = await gameSnapshot(request, gameId, p1.session);
    expect(firstDraft.pending_choice?.choice.player).toBe(p1.id);
    expect((await gameSnapshot(request, gameId, p2.session)).pending_choice).toBeFalsy();
    expect((await gameSnapshot(request, gameId, p3.session)).pending_choice).toBeFalsy();

    // Assert options in P1 modal are actionable
    const p1Options = pageP1.locator('[data-testid="choice-option"]');
    const optCount = await p1Options.count();
    expect(optCount).toBeGreaterThan(0);
    for (let i = 0; i < optCount; i++) {
      await expect(p1Options.nth(i)).toHaveAttribute("data-actionable", "true");
    }

    // Test Choice Minimization: P1 minimizes dialog to inspect map
    await pageP1.locator('[data-testid="minimize-choice-button"]').click();
    await expect(p1Modal).toHaveCount(0);
    const minBanner = pageP1.locator('[data-testid="minimized-choice-banner"]');
    await expect(minBanner).toBeVisible();

    // Map and board SVG are completely accessible while decision is minimized
    const boardSvg = pageP1.locator('[data-testid="ti4-board-svg"]');
    await expect(boardSvg).toBeVisible();
    await expect(pageP1.locator('[data-testid="system-hex-18"]')).toBeVisible();
    await expect(pageP1.locator('button[title="Zoom In"]')).toBeVisible();

    // Verify Event Log toggle and entries have timestamps while inspecting
    const eventLogToggle = pageP1.locator('[data-testid="event-log-toggle"]');
    await eventLogToggle.click();
    const logList = pageP1.locator('[data-testid="event-log-list"]');
    await expect(logList).toBeVisible();
    const firstEntry = pageP1.locator('[data-testid="event-log-entry"]').first();
    await expect(firstEntry).toBeVisible();
    const entryText = await firstEntry.innerText();
    expect(entryText).toMatch(/\d{2}:\d{2}:\d{2}/);
    // Close event log drawer
    await eventLogToggle.click();
    await expect(logList).toHaveCount(0);

    // Restore dialog
    await pageP1.locator('[data-testid="resume-choice-button"]').click();
    await expect(p1Modal).toBeVisible();
    await expect(minBanner).toHaveCount(0);

    // Verify P1 has human-readable secret objective with description and tooltip
    const p1Secret = pageP1.locator('[data-testid^="secret-objective-item-"]');
    await expect(p1Secret).toBeVisible();
    await expect(p1Secret).toContainText("Forge an Alliance");
    await expect(p1Secret).toContainText("Control 4 cultural planets.");
    const soTitle = await p1Secret.getAttribute("title");
    expect(soTitle).toContain("Forge an Alliance");
    expect(soTitle).toContain("Control 4 cultural planets.");

    // P1 submits choice (first option: Leadership)
    await pageP1.locator('[data-testid="submit-choice-button"]').click();

    // P1 modal closes
    await expect(p1Modal).toHaveCount(0);

    // P1 now displays human-readable strategy card badge with tooltip
    const scBadge = pageP1.locator('[data-testid="strategy-card-badge-pok1leadership"]');
    await expect(scBadge).toBeVisible();
    await expect(scBadge).toContainText("1. Leadership");
    const scTitle = await scBadge.getAttribute("title");
    expect(scTitle).toContain("Gain 3 command tokens");

    // Broadcast propagates: P2 now receives choice!
    await expect(p2Modal).toBeVisible({ timeout: 5000 });
    await expect(pageP3.getByTestId("pending-choice-dialog")).toHaveCount(0);
    const secondDraft = await gameSnapshot(request, gameId, p2.session);
    expect(secondDraft.game_version).toBeGreaterThan(firstDraft.game_version);
    expect(secondDraft.pending_choice?.choice.player).toBe(p2.id);
    expect((await gameSnapshot(request, gameId, p1.session)).pending_choice).toBeFalsy();
    expect((await gameSnapshot(request, gameId, p3.session)).pending_choice).toBeFalsy();

    // Assert P2 options are actionable
    const p2Options = pageP2.locator('[data-testid="choice-option"]');
    const p2OptCount = await p2Options.count();
    expect(p2OptCount).toBeGreaterThan(0);
    await expect(pageP2.getByTestId("submit-choice-button")).toBeEnabled();

    // P2 submits choice
    await pageP2.locator('[data-testid="submit-choice-button"]').click();

    // The third human now has the draft. Reconnect while their decision is pending.
    await expect(p2Modal).toHaveCount(0);
    const p3Modal = pageP3.getByTestId("pending-choice-dialog");
    await expect(p3Modal).toBeVisible();
    await expect(pageP1.getByTestId("pending-choice-dialog")).toHaveCount(0);
    await expect(pageP2.getByTestId("submit-choice-button")).toHaveCount(0);
    const p3BeforeReconnect = await gameSnapshot(request, gameId, p3.session);
    expect(p3BeforeReconnect.game_version).toBeGreaterThan(secondDraft.game_version);
    expect(p3BeforeReconnect.pending_choice?.choice.player).toBe(p3.id);
    const p3Nonce = p3BeforeReconnect.pending_choice?.nonce;
    await pageP3.reload();
    await expect(pageP3.getByTestId("connection-indicator")).toHaveAttribute(
      "data-status",
      "connected",
    );
    await expect(p3Modal).toBeVisible();
    await expect(pageP3.getByTestId("submit-choice-button")).toBeEnabled();
    expect((await gameSnapshot(request, gameId, p3.session)).pending_choice?.nonce).toBe(p3Nonce);
    await pageP3.getByTestId("minimize-choice-button").click();
    await expect(pageP3.getByTestId("minimized-choice-banner")).toBeVisible();
    await expect(pageP3.getByTestId("resume-choice-button")).toBeEnabled();
    await pageP3.getByTestId("resume-choice-button").click();
    await expect(p3Modal).toBeVisible();

    // Resolve each remaining human draft pick, checking the server's next seat
    // before interacting with that seat's private controls.
    let draftPicks = 2;
    while (true) {
      const current = await gameSnapshot(request, gameId, p1.session);
      if (current.view.phase.toLowerCase() === "action") break;
      expect(
        draftPicks,
        `draft did not reach Action: ${JSON.stringify(current.turn_status)}`,
      ).toBeLessThan(8);
      expect(current.turn_status.kind).toBe("waiting_for_decision");
      if (current.turn_status.kind !== "waiting_for_decision")
        throw new Error("draft has no acting seat");
      const index = players.findIndex((player) => player.id === current.turn_status.seat);
      expect(index, `unexpected draft seat: ${current.turn_status.seat}`).toBeGreaterThanOrEqual(0);
      const actor = pages[index];
      const privateState = await gameSnapshot(request, gameId, players[index].session);
      expect(privateState.pending_choice?.choice.context?.subtype).toBe("draft_strategy_card");
      expect(privateState.pending_choice?.choice.player).toBe(players[index].id);
      for (const [otherIndex, otherPage] of pages.entries()) {
        if (otherIndex === index) continue;
        await expect(otherPage.getByTestId("pending-choice-dialog")).toHaveCount(0);
        await expect(otherPage.getByTestId("submit-choice-button")).toHaveCount(0);
        expect(
          (await gameSnapshot(request, gameId, players[otherIndex].session)).pending_choice,
        ).toBeFalsy();
      }
      await expect(actor.getByTestId("pending-choice-dialog")).toBeVisible();
      await expect(actor.getByTestId("submit-choice-button")).toBeEnabled();
      const before = privateState.game_version;
      await actor.getByTestId("submit-choice-button").click();
      await expect
        .poll(
          async () => {
            const next = await gameSnapshot(request, gameId, p1.session);
            return (
              next.game_version > before &&
              (next.view.phase.toLowerCase() === "action" ||
                (next.turn_status.kind === "waiting_for_decision" &&
                  (next.turn_status.seat !== players[index].id ||
                    (await gameSnapshot(request, gameId, players[index].session)).pending_choice
                      ?.nonce !== privateState.pending_choice?.nonce)))
            );
          },
          { message: `draft pick ${draftPicks + 1} did not advance from ${players[index].id}` },
        )
        .toBe(true);
      draftPicks++;
    }

    expect(draftPicks).toBeGreaterThanOrEqual(3);
    for (const [index, page] of pages.entries()) {
      await expect
        .poll(async () =>
          (await gameSnapshot(request, gameId, players[index].session)).view.phase.toLowerCase(),
        )
        .toBe("action");
      await expect(page.getByTestId("turn-status-bar")).toContainText("action Phase", {
        ignoreCase: true,
      });
    }
    await assertPageInvariants(pageP1, 1);
    await assertPageInvariants(pageP2, 2);
    await assertPageInvariants(pageP3, 3);

    const actionState = await gameSnapshot(request, gameId, p1.session);
    expect(actionState.turn_status.kind).toBe("waiting_for_decision");
    if (actionState.turn_status.kind !== "waiting_for_decision")
      throw new Error("Action has no acting seat");
    const actionIndex = players.findIndex((player) => player.id === actionState.turn_status.seat);
    expect(actionIndex).toBeGreaterThanOrEqual(0);
    const actionPage = pages[actionIndex];
    const actorState = await gameSnapshot(request, gameId, players[actionIndex].session);
    expect(actorState.pending_choice?.choice.player).toBe(players[actionIndex].id);
    const tactical = actorState.pending_choice?.choice.options.find(
      (option) => option.id === "tactical",
    );
    expect(
      tactical,
      `first Action options: ${JSON.stringify(actorState.pending_choice?.choice.options)}`,
    ).toBeDefined();
    for (const [index, page] of pages.entries()) {
      if (index === actionIndex) continue;
      await expect(page.getByTestId("pending-choice-dialog")).toHaveCount(0);
      expect(
        (await gameSnapshot(request, gameId, players[index].session)).pending_choice,
      ).toBeFalsy();
    }
    await expect(actionPage.getByTestId("pending-choice-dialog")).toBeVisible();
    await actionPage.locator('[data-testid="choice-option"][data-option-id="tactical"]').click();
    await expect(actionPage.getByTestId("submit-choice-button")).toBeEnabled();
    await actionPage.getByTestId("submit-choice-button").click();
    await expect
      .poll(
        async () => {
          const next = await gameSnapshot(request, gameId, players[actionIndex].session);
          return (
            next.game_version > actorState.game_version &&
            next.view.phase.toLowerCase() === "action" &&
            next.pending_choice?.nonce !== actorState.pending_choice?.nonce &&
            next.pending_choice?.choice.context?.subtype === "activate_system"
          );
        },
        { message: "tactical action did not advance to an authoritative activate_system decision" },
      )
      .toBe(true);
    await expect
      .poll(async () => Number((await actionPage.getByTestId("game-version").innerText()).slice(1)))
      .toBeGreaterThan(actorState.game_version);

    // Disconnect & Reconnect Invariant Test
    await pageP1.goto("/");
    await expect(pageP1.locator('[data-testid="lobby-container"]')).toBeVisible();

    // Rejoin as p1 with its tab-scoped current credential.
    await openPlayerGame(pageP1, gameId, p1.session);

    await expect(pageP1.locator('[data-testid="turn-status-bar"]')).toBeVisible();
    await assertPageInvariants(pageP1, 1);
    await assertPageInvariants(pageP2, 2);
    await assertPageInvariants(pageP3, 3);
  });
});
