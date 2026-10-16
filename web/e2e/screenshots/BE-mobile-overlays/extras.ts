import type { Page } from "@playwright/test";
import { GAME_ID, openMockedGame } from "../_shared/mockGame";
import { actionCardEventLog, actor, completedCombatBoard, galleryBoard, systemActivationOptions } from "../_shared/fixtures";
import { opponent, playerWithHand } from "../_shared/players";
import { openWaiting } from "../AH-secondary-prep-real-ui/prep";
import { groundCombatBoard, outcomes } from "../T-ground-combat-summary/fixture";
import { publicEntry, pushEntry } from "../G-corner-notifications/corner";
import { PROTOCOL_VERSION } from "../../../src/protocol/types";
import { SIZES, type SizeName } from "./scene";

const players = () => [playerWithHand(), opponent];

/** Real-flow and user-opened overlays that the decision gallery does not produce. */
export interface Extra {
  id: string;
  title: string;
  open: (page: Page) => Promise<void>;
}

/** The Events control of this layout: the phone drawer button, or the docked panel's toggle on a wide screen. */
const eventsToggle = async (page: Page) => {
  const phone = page.getByTestId("event-log-mobile-toggle");
  return (await phone.isVisible()) ? phone : page.getByTestId("event-log-toggle");
};

const ready = async (page: Page) => {
  await page.getByTestId("ti4-board-svg").waitFor();
  await page.waitForTimeout(350);
};

const planets = [
  { id: "jord", label: "Jord", resources: 2, influence: 2, traits: ["cultural"] },
  { id: "moll", label: "Moll", resources: 1, influence: 3, traits: ["hazardous"] },
];

/** The landing tray of an invasion of system 18 (copied from the AX scene). */
async function landing(page: Page) {
  const base = galleryBoard.systems["18"];
  const units = Array.from({ length: 5 }, () => ({ owner: actor, unit_type: "infantry", damaged: false }));
  const board = {
    ...galleryBoard,
    map_tiles: galleryBoard.map_tiles?.map((t) => (t.system_id === "18" ? { ...t, planets } : t)),
    systems: {
      ...galleryBoard.systems,
      "18": {
        ...base,
        units,
        planets: Object.fromEntries(planets.map((p) => [p.id, { planet_id: p.id, exhausted: false }])),
      },
    },
    invasion: {
      system_id: "18",
      invasion_seq: 1,
      invader: actor,
      phase: "landing" as const,
      planets: planets.map((p) => p.id),
      current_planet: null,
      defender: null,
      ground_round: 0,
      odds_context: {},
    },
  };
  await openMockedGame(page, {
    players: players(),
    board,
    choice: {
      prompt: "Land ground forces",
      context: { subtype: "commit_ground_forces", target: { System: "18" } },
      options: [
        ...planets.map((p) => ({ id: `land|${p.id}|infantry`, label: `Land infantry on ${p.label}`, kind: "land", payload: { planet: p.id, unit: "infantry" } })),
        { id: "done_committing", label: "Done committing", kind: "decline" },
      ],
    },
  });
  await page.getByTestId("invasion-planet-values").first().waitFor();
}

export const extras: Extra[] = [
  {
    id: "objectives",
    title: "Objectives modal (header button)",
    open: async (page) => {
      await openMockedGame(page, { players: players() });
      await ready(page);
      await page.getByTestId("objectives-modal-button").click();
      await page.waitForTimeout(300);
    },
  },
  {
    id: "technologies",
    title: "Technologies modal (header button)",
    open: async (page) => {
      await openMockedGame(page, { players: players() });
      await ready(page);
      await page.getByTestId("technology-modal-button").click();
      await page.waitForTimeout(300);
    },
  },
  {
    id: "event-log",
    title: "Event log drawer",
    open: async (page) => {
      await openMockedGame(page, { players: players(), events: actionCardEventLog() });
      await ready(page);
      await (await eventsToggle(page)).evaluate((el) => (el as HTMLElement).click());
      await page.waitForTimeout(400);
    },
  },
  {
    id: "player-sheet",
    title: "Player sheet drawer",
    open: async (page) => {
      await openMockedGame(page, { players: players() });
      await ready(page);
      await page.getByTestId("player-sheet-toggle").click({ timeout: 3000 });
      await page.waitForTimeout(400);
    },
  },
  {
    id: "system-inspector",
    title: "System detail panel (tap a system)",
    open: async (page) => {
      await openMockedGame(page, {
        players: players(),
        choice: { prompt: "Choose a system to activate", context: { subtype: "activate_system" }, options: systemActivationOptions },
      });
      await ready(page);
      await page.getByTestId("system-hex-18").click();
      await page.getByTestId("system-inspector").waitFor();
    },
  },
  {
    id: "card-details",
    title: "Card detail panel (strategy card from the player sheet)",
    open: async (page) => {
      await openMockedGame(page, { players: players() });
      await ready(page);
      await page.getByTestId("player-sheet-toggle").click({ timeout: 3000 });
      await page.locator('[data-testid^="strategy-card-badge-"]').first().click();
      await page.getByTestId("detail-panel").waitFor();
      // The panel is the subject here, not the sheet it came from: close the drawer (its toggle stays on top).
      await page.getByTestId("player-sheet-toggle").click();
      await page.waitForTimeout(300);
    },
  },
  {
    id: "combat-result",
    title: "Finished space combat (result modal)",
    open: async (page) => {
      await openMockedGame(page, { players: players(), board: completedCombatBoard() });
      await ready(page);
      await page.waitForTimeout(1200);
    },
  },
  {
    id: "invasion-landing",
    title: "Invasion landing tray",
    open: landing,
  },
  {
    id: "tile-tooltip",
    title: "System tooltip (touch on a system, no decision)",
    open: async (page) => {
      await openMockedGame(page, { players: players() });
      await ready(page);
      await page.getByTestId("system-hex-18").hover({ force: true });
      await page.waitForTimeout(400);
    },
  },
  {
    id: "undo-confirm",
    title: "Undo confirmation dialog",
    open: async (page) => {
      await openMockedGame(page, { players: players(), events: actionCardEventLog(), history: { cursor: 5, redo_count: 0 } });
      await ready(page);
      await (await eventsToggle(page)).evaluate((el) => (el as HTMLElement).click());
      await page.getByTestId("event-log-list").waitFor();
      const list = page.getByTestId("event-log-list");
      for (let i = 0; i < 6; i++) {
        const closed = list.locator('button[aria-expanded="false"]');
        if (!(await closed.count())) break;
        await closed.first().click();
      }
      await page.getByRole("button", { name: /Undo from decision/ }).first().evaluate((el) => (el as HTMLElement).click());
      await page.getByTestId("undo-confirm-dialog").waitFor();
    },
  },
  {
    id: "history-error",
    title: "Error toast (a refused history change)",
    open: async (page) => {
      await page.route(`**/api/games/${GAME_ID}/history`, (route) => route.fulfill({ status: 500, body: "The server could not rewind the game." }));
      await openMockedGame(page, { players: players(), events: actionCardEventLog(), history: { cursor: 5, redo_count: 0 } });
      await ready(page);
      await (await eventsToggle(page)).evaluate((el) => (el as HTMLElement).click());
      await page.getByTestId("event-log-list").waitFor();
      const list = page.getByTestId("event-log-list");
      for (let i = 0; i < 6; i++) {
        const closed = list.locator('button[aria-expanded="false"]');
        if (!(await closed.count())) break;
        await closed.first().click();
      }
      await page.getByRole("button", { name: /Undo from decision/ }).first().evaluate((el) => (el as HTMLElement).click());
      await page.getByTestId("undo-confirm-accept").click();
      await page.locator(".session-error").waitFor();
      // The error toast lies over the Events button (that is what this scene shows): click through the DOM.
      await (await eventsToggle(page)).evaluate((el) => (el as HTMLElement).click());
      await page.waitForTimeout(300);
    },
  },
  {
    id: "ground-combat-result",
    title: "Ground combat result (invasion overlay)",
    open: async (page) => {
      await openMockedGame(page, { players: players(), board: groundCombatBoard(outcomes.attackerWins) });
      await ready(page);
      await page.waitForTimeout(800);
    },
  },
  {
    id: "corner-toast",
    title: "Corner toast (other player's action)",
    open: async (page) => {
      const game = await openMockedGame(page, { players: players() });
      await ready(page);
      pushEntry(game, publicEntry("toast-1", opponent.id, "played Direct Hit on your Dreadnought in Mecatol Rex"));
      await page.getByTestId("corner-toasts").waitFor();
      await page.waitForTimeout(300);
    },
  },
  {
    id: "prep-chip",
    title: "Secondary prep chip",
    open: async (page) => {
      await openWaiting(page, { name: "Technology", card: "pok7technology", viewer: { trade_goods: 5 } });
      await page.getByTestId("secondary-prep-chip").waitFor();
      await page.waitForTimeout(300);
    },
  },
  {
    id: "prep-banner",
    title: "Secondary prepare banner (prepare mode)",
    open: async (page) => {
      await openWaiting(page, { name: "Technology", card: "pok7technology", viewer: { trade_goods: 5 } });
      await page.getByTestId("secondary-prep-chip").evaluate((el) => (el as HTMLElement).click());
      await page.getByTestId("prepare-banner").waitFor();
      await page.waitForTimeout(300);
    },
  },
  {
    id: "prep-technology",
    title: "Secondary prep: technology dialog",
    open: async (page) => {
      await openWaiting(page, { name: "Technology", card: "pok7technology", viewer: { trade_goods: 5 } });
      await page.getByTestId("secondary-prep-chip").evaluate((el) => (el as HTMLElement).click());
      await page.getByTestId("prepare-banner").waitFor();
      await page.getByTestId("secondary-yes-btn").click();
      await page.waitForTimeout(500);
    },
  },
  {
    id: "waiting-other",
    title: "Waiting for another seat (no overlay)",
    open: async (page) => {
      await openMockedGame(page, { seat: opponent.id, players: players(), turnStatus: { kind: "waiting_for_reactions", phase: "action", round: 2 } });
      await ready(page);
    },
  },
  {
    id: "game-over",
    title: "Game over banner",
    open: async (page) => {
      const game = await openMockedGame(page, { players: players(), phase: "status", round: 5 });
      await ready(page);
      game.send({ type: "game_over", protocol_version: PROTOCOL_VERSION, game_id: GAME_ID, game_version: 41, winner: actor, final_scores: { [actor]: 10, [opponent.id]: 9 } });
      await page.waitForTimeout(400);
    },
  },
];

export async function openExtra(page: Page, id: string, size: SizeName) {
  await page.setViewportSize(SIZES[size]);
  await extras.find((e) => e.id === id)!.open(page);
}
