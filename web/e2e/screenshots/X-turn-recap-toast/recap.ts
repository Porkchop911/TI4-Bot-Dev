import type { Page } from "@playwright/test";
import type { GameEvent } from "../../../src/protocol/types";
import { publicEntry, pushEntry } from "../G-corner-notifications/corner";
import type { MockedGame } from "../_shared/mockGame";
import { actor } from "../_shared/players";

export { openCornerGame, cornerCrop, pushEntry } from "../G-corner-notifications/corner";

/** The key the viewer-local recap toggle is stored under (useTurnRecapSetting). */
export const TURN_RECAP_KEY = "player_turn_recap";

/** Turns the recap on before the app loads, as a viewer who enabled it earlier would have it. */
export async function recapAlreadyOn(page: Page) {
  await page.addInitScript((key) => localStorage.setItem(key, "true"), TURN_RECAP_KEY);
}

/** The opponent's seat id in the mocked game. */
export const OTHER = "other_seat";

/** A public entry of the action phase that belongs to one action (`actionId`). */
export function step(
  id: string,
  actionId: string,
  actor: string,
  what: string | undefined,
  extra: Partial<GameEvent> = {},
): GameEvent {
  const base = publicEntry(id, actor, what ?? "", { action_id: actionId, action_actor: OTHER, ...extra });
  if (what === undefined) delete (base as { detail?: string }).detail;
  return base;
}

/** The actor's menu choice, which starts an action: tactical, strategic, component or pass. */
export const selection = (id: string, actionId: string, actor: string, type: string) =>
  step(id, actionId, actor, undefined, { stage: "action selection", action_type: type, action_actor: actor });

/** Pushes entries one after the other, as the server delivers them while a turn plays out. */
export function pushAll(game: MockedGame, entries: GameEvent[]) {
  for (const entry of entries) pushEntry(game, entry);
}

/** A tactical action: activation, three ships moved, a combat and production. */
export const tacticalTurn = (actionId: string, seat = OTHER): GameEvent[] => [
  selection(`${actionId}-0`, actionId, seat, "tactical"),
  step(`${actionId}-1`, actionId, seat, "activated #27", { stage: "activation" }),
  step(`${actionId}-2`, actionId, seat, "moved cruiser from #1 to #27", { stage: "movement" }),
  step(`${actionId}-3`, actionId, seat, "moved cruiser from #1 to #27", { stage: "movement" }),
  step(`${actionId}-4`, actionId, seat, "moved carrier from #3 to #27", { stage: "movement" }),
  step(`${actionId}-5`, actionId, actor, "lost a fighter in #27", { stage: "combat" }),
  step(`${actionId}-6`, actionId, seat, "lost a fighter in #27", { stage: "combat" }),
  step(`${actionId}-7`, actionId, seat, "produced 2 fighter", { stage: "production" }),
];

/** A strategic action. */
export const strategicTurn = (actionId: string, seat = OTHER): GameEvent[] => [
  selection(`${actionId}-0`, actionId, seat, "strategic"),
  step(`${actionId}-1`, actionId, seat, "gained a tactic command token", { stage: "strategy" }),
  step(`${actionId}-2`, actionId, seat, "gained a fleet command token", { stage: "strategy" }),
  step(`${actionId}-3`, actionId, seat, "researched Neural Motivator", { stage: "strategy" }),
];
