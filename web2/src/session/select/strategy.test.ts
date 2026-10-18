import { expect, it } from "vitest";
import menuUpdate from "../../../../fixtures/protocol/update-menu.json";
import { informs, type BlockView, type FlowActionView } from "../../model";
import type { SessionUpdate } from "../wire";
import type { LocalState } from "./action";
import { selectShell } from "./shell";

// Warfare of Hacan (b) in a real update of the engine: Letnev (c) followed, the viewer (a) is
// asked its secondary, Xxcha (d) and Jol-Nar (e) wait. Without the catalogue a faction is named by its id.
const base = menuUpdate as unknown as SessionUpdate;
const local: LocalState = {
  inspected: null,
  staged: null,
  sent: null,
  error: null,
  replaying: null,
  canUndo: false,
  movement: {},
  landing: {},
  landingKind: null,
  production: { builds: [], planets: [], goods: 0 },
  productionStage: "build",
  handled: [],
  step: null,
  remaining: null,
  planNote: null,
  stepping: null,
  draftLocked: null,
  draft: null,
};
const warfare = (asked: boolean): SessionUpdate => ({
  ...base,
  secondary: {
    primary: "b",
    card: "pok6warfare",
    name: "Warfare",
    open: true,
    followers: [
      { seat: "c", state: "followed" },
      { seat: "a", state: "waiting" },
      { seat: "d", state: "waiting" },
      { seat: "e", state: "waiting" },
    ],
    draft: false,
  },
  pending_choice: asked
    ? {
        nonce: "follow",
        choice: {
          ...base.pending_choice!.choice,
          prompt: "Follow Warfare?",
          context: {
            ...base.pending_choice!.choice.context!,
            subtype: "strategy_secondary",
            source: { StrategyCard: { card: "pok6warfare", secondary: true } },
          },
          details: { kind: "strategy_secondary", costs_token: false },
          options: [
            { id: "follow", kind: "yes", label: "follow", payload: {} },
            { id: "decline", kind: "decline", label: "decline", payload: {} },
          ],
        },
      }
    : undefined,
  view: { ...base.view, active_player: "b" },
});
const flow = (update: SessionUpdate): FlowActionView => {
  const { action } = selectShell(update, local);
  if (action.kind !== "flow") {
    throw new Error("no strategic action");
  }
  return action;
};
const seats = (blocks: BlockView[]) =>
  blocks.flatMap((block) =>
    block.kind === "seats"
      ? [block.rows.map((row) => `${row.order} ${row.status.label}${row.open ? " ▾" : ""}`)]
      : [],
  );

it("asks the viewer's secondary: the viewer's row with its choice decides, the other seats inform", () => {
  const view = flow(warfare(true));
  const decide = view.blocks.filter((block) => !informs(block));
  const inform = view.blocks.filter(informs);
  // The step sheet: the row of the viewer, open into the list of the game.
  expect(seats(decide)).toEqual([["2 Your decision ▾"]]);
  const open = decide[0].kind === "seats" ? decide[0].rows[0].open : undefined;
  expect(open?.at(-1)).toMatchObject({ kind: "menu" });
  // The Log: every seat that did something or decides, the viewer's row with nothing open, and
  // the seats that wait in one line.
  expect(seats(inform)).toEqual([["P Resolved", "1 Followed", "2 Your decision"]]);
  expect(inform.at(-1)).toMatchObject({ kind: "note", text: "Then: Xxcha, Jolnar" });
});

it("informs only while the viewer has no question: one list of the seats", () => {
  const view = flow(warfare(false));
  expect(view.blocks.every(informs)).toBe(true);
  expect(seats(view.blocks)).toEqual([["P Resolved", "1 Followed", "2 Deciding"]]);
});
