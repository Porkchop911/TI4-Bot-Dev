import { expect, type Page, type TestInfo } from "@playwright/test";
import { mkdirSync, writeFileSync } from "node:fs";
import { join } from "node:path";
import { openMockedGame, type MockGameOptions } from "../_shared/mockGame";
import { actionCardEventLog, galleryDecision } from "../_shared/fixtures";
import { playerWithHand, opponent } from "../_shared/players";
import { artifactDir } from "../_shared/shot";
import { conflict, handoff, newTurn, reservedCard, routeTurnRedoStatus, holdTurnRedoPost } from "../Q-turn-redo/redoMock";
import type { TurnRedoStatus } from "../../../src/protocol/turnRedo";

export const prefix = process.env.TI4_SHOT_PREFIX ?? "after";

export const phones = [
  { id: "390x844", width: 390, height: 844 },
  { id: "360x740", width: 360, height: 740 },
  { id: "412x915", width: 412, height: 915 },
  { id: "844x390", width: 844, height: 390 },
];

/** Every recorded decision was kept and the round ended without a stop (the "complete" state). */
const completeStatus: TurnRedoStatus = {
  ...handoff,
  outcome: { ...handoff.outcome!, stop: { kind: "tail_exhausted" } },
};

const turnBar: MockGameOptions["choice"] = {
  prompt: "action phase",
  options: [
    { id: "tactical", label: "Tactical action", kind: "action" },
    { id: "pass", label: "Pass", kind: "action" },
  ],
  details: { kind: "turn_menu", closing: false, tokens: { tactic: 3, fleet: 3, strategy: 2 }, strategy_cards: [], partners: [] },
};

export interface Scene {
  id: string;
  status: TurnRedoStatus | null;
  hold?: boolean;
  choice?: MockGameOptions["choice"];
  state?: string;
  /** Opens the phone event log drawer instead of looking at the strip. */
  log?: boolean;
}

export const scenes: Scene[] = [
  { id: "1-log-entry", status: null, log: true },
  { id: "2-rewinding", status: null, hold: true, state: "rewinding" },
  { id: "3-new-turn", status: newTurn, state: "new-turn" },
  { id: "4-replaying", status: { ...newTurn, turn_complete: true }, hold: true, state: "replaying" },
  { id: "5-handoff", status: handoff, state: "handoff" },
  { id: "6-complete", status: completeStatus, state: "complete" },
  { id: "7-conflict", status: conflict, state: "conflict" },
  { id: "8-reserved-card", status: reservedCard, state: "conflict" },
  { id: "9-with-decision", status: handoff, state: "handoff", choice: galleryDecision("Empty movement") },
  { id: "10-with-turn-bar", status: conflict, state: "conflict", choice: turnBar },
  { id: "11-replaying-turn-bar", status: { ...newTurn, turn_complete: true }, hold: true, state: "replaying", choice: turnBar },
];

/** Opens the event log (the Events button on a phone, the docked toggle otherwise) and expands every node. */
async function openLog(page: Page) {
  const phoneToggle = page.getByTestId("event-log-mobile-toggle");
  if (await phoneToggle.isVisible()) await phoneToggle.click();
  else await page.getByTestId("event-log-toggle").click();
  const list = page.getByTestId("event-log-list");
  await list.waitFor();
  for (let i = 0; i < 6; i++) {
    const closed = list.locator('button[aria-expanded="false"]');
    if (!(await closed.count())) break;
    await closed.first().click();
  }
}

export async function openScene(page: Page, scene: Scene) {
  await routeTurnRedoStatus(page, scene.status);
  if (scene.hold) await holdTurnRedoPost(page);
  await openMockedGame(page, {
    players: [playerWithHand(), opponent],
    events: actionCardEventLog(),
    phase: "action",
    ...(scene.choice ? { choice: scene.choice } : {}),
  });
  if (scene.log) {
    await openLog(page);
    await expect(page.getByTestId("turn-redo-btn")).toBeVisible();
    return;
  }
  if (scene.id === "2-rewinding") {
    await openLog(page);
    // On the base commit the turn action bar lies over this button on a phone, and in landscape the
    // header lies over the log toggle: click through the DOM so the redo can still be started.
    await page.getByTestId("turn-redo-btn").evaluate((el) => (el as HTMLElement).click());
    // The phone drawer covers the strip: close it again.
    const phoneToggle = page.getByTestId("event-log-mobile-toggle");
    if (await phoneToggle.isVisible()) await phoneToggle.click();
  }
  const bar = page.getByTestId("turn-redo-bar");
  await expect(bar).toBeVisible();
  if (scene.state) await expect(bar).toHaveAttribute("data-state", scene.state);
}

export type Box = { x: number; y: number; width: number; height: number };
export const overlaps = (a: Box, b: Box) =>
  a.x < b.x + b.width - 0.5 && b.x < a.x + a.width - 0.5 && a.y < b.y + b.height - 0.5 && b.y < a.y + a.height - 0.5;

export const SELECTORS = {
  bar: '[data-testid="turn-redo-bar"]',
  sheet: '[data-testid="turn-redo-sheet"]',
  header: ".app-shell__header",
  actions: ".app-shell__mobile-actions",
  turnBar: '[data-testid="turn-action-bar"]',
  toolbar: ".map-overlay-toolbar",
  legend: ".board-seat-legend",
  dialog: ".decision-dialog, [role=dialog]",
  restore: '[data-testid="turn-redo-restore"]',
  keep: '[data-testid="turn-redo-keep"]',
  log: "#event-log-drawer",
};

export async function boxes(page: Page) {
  const out: Record<string, Box | null> = {};
  for (const [key, selector] of Object.entries(SELECTORS)) {
    const loc = page.locator(selector).first();
    out[key] = (await loc.count()) && (await loc.isVisible()) ? await loc.boundingBox() : null;
  }
  return out;
}

export async function measure(page: Page, testInfo: TestInfo, name: string) {
  const vp = page.viewportSize()!;
  const out = await boxes(page);
  const pieces = [out.bar, out.sheet].filter((b): b is Box => !!b);
  const line: Record<string, unknown> = { name, viewport: `${vp.width}x${vp.height}` };
  if (pieces.length) {
    const area = pieces.reduce((sum, b) => sum + b.width * b.height, 0);
    line.stripHeightPx = Math.round(out.bar!.height);
    line.sheetHeightPx = out.sheet ? Math.round(out.sheet.height) : null;
    line.heightFrac = +(Math.max(...pieces.map((b) => b.height)) / vp.height).toFixed(3);
    line.areaFrac = +(area / (vp.width * vp.height)).toFixed(3);
    line.covers = Object.entries(out)
      .filter(([k, b]) => k !== "bar" && k !== "sheet" && k !== "log" && b && pieces.some((piece) => overlaps(piece, b)))
      .map(([k]) => k);
  }
  const dir = join(artifactDir(testInfo), "metrics");
  mkdirSync(dir, { recursive: true });
  writeFileSync(join(dir, `${prefix}-${name.replace(/ /g, "-")}.json`), JSON.stringify(line) + "\n");
  return { boxes: out, vp, line };
}
