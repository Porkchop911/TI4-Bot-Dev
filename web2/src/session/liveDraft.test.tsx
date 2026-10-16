import { act, renderHook } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import activationUpdate from "./fixtures/update-activation.json";
import menuUpdate from "./fixtures/update-menu.json";
import movementUpdate from "./fixtures/update-movement.json";
import type { Transport, TransportEvent } from "./transport";
import { type LiveSession, useLiveSession } from "./useLiveSession";
import type { DraftOutcome, DraftStep, SessionUpdate } from "./wire";

// The outcome of a draft comes later than it is asked for (`Transport.draft`). Updates of the
// real engine: the turn menu of seat a, and the activation and the movement of a tactical action.
const menu = menuUpdate as unknown as SessionUpdate;
const draftOf = (update: unknown): SessionUpdate => {
  const made = update as SessionUpdate;
  return { ...made, pending_choice: { ...made.pending_choice!, nonce: "draft" } };
};
const activation: DraftOutcome = {
  update: draftOf(activationUpdate),
  consumed: 1,
  stop: { kind: "open" },
};
const movement: DraftOutcome = {
  update: draftOf(movementUpdate),
  movement: draftOf(movementUpdate),
  consumed: 2,
  stop: { kind: "open" },
};

function transport() {
  let listener: (event: TransportEvent) => void = () => {};
  const asked: DraftStep[][] = [];
  const made = {
    catalog: null,
    capabilities: {},
    subscribe: (to: typeof listener) => {
      listener = to;
      return () => {};
    },
    draft: (script: DraftStep[]) => void asked.push(script),
    submitChoice: vi.fn(),
    submitPlan: vi.fn(),
  };
  const tell = (event: TransportEvent) => act(async () => listener(event));
  return { made, asked, transport: made as unknown as Transport, tell };
}

const main = (live: LiveSession) => {
  const action = live.session!.view.action;
  const footer = action.kind === "flow" ? action.footer : action.task.footer;
  return footer.actions.find((button) => button.tone === "primary");
};
const step = (live: LiveSession) => {
  const action = live.session!.view.action;
  return action.kind === "tactical" ? action.task.title : action.kind;
};

it("keeps the last outcome of a draft until the next one arrives, and drops a late one", async () => {
  const game = transport();
  const { result } = renderHook(() => useLiveSession(game.transport));
  const dispatch = (intent: Parameters<NonNullable<LiveSession["session"]>["dispatch"]>[0]) =>
    act(async () => result.current.session!.dispatch(intent));
  await game.tell({ kind: "update", update: menu, canUndo: false });
  expect(result.current.session?.view.toolbar.workspace).toBe("live");

  // The draft opens before the game has said anything about it: nothing of the live game can
  // be sent from it.
  await dispatch({ type: "workspace", mode: "draft" });
  expect(game.asked).toEqual([[{ kind: "choose", option_id: "tactical" }]]);
  expect(result.current.session?.view.toolbar.workspace).toBe("draft");
  expect(main(result.current)).toBeUndefined();

  await game.tell({ kind: "draft", script: game.asked[0], outcome: activation });
  const atActivation = step(result.current);
  expect(main(result.current)).toMatchObject({ label: "Activate system", disabled: true });
  await dispatch({ type: "toggleSystem", system: "23" });
  expect(main(result.current)?.disabled).toBeFalsy();

  // The activation is recorded: the game is asked again. Until it answers, the screen is of
  // the outcome before, and its main button waits.
  await dispatch({ type: "commitEdit" });
  expect(game.asked).toHaveLength(2);
  expect(game.asked[1]).toEqual([
    { kind: "choose", option_id: "tactical" },
    { kind: "choose", option_id: "23" },
  ]);
  expect(step(result.current)).toBe(atActivation);
  expect(main(result.current)?.disabled).toBe(true);
  expect(result.current.session?.view.toolbar.draft?.canApply).toBe(false);

  // An answer for the script before is late: it is dropped.
  await game.tell({
    kind: "draft",
    script: game.asked[0],
    outcome: { ...activation, stop: { kind: "failed", reason: "late" } },
  });
  expect(step(result.current)).toBe(atActivation);
  expect(main(result.current)?.disabled).toBe(true);

  await game.tell({ kind: "draft", script: game.asked[1], outcome: movement });
  expect(step(result.current)).not.toBe(atActivation);
  expect(main(result.current)).toMatchObject({ label: "Move nothing" });
  expect(main(result.current)?.disabled).toBeFalsy();

  // Nothing of this was sent to the game.
  expect(game.made.submitChoice).not.toHaveBeenCalled();
  expect(game.made.submitPlan).not.toHaveBeenCalled();
});

it("asks for the draft again when the game changes, and keeps what is staged and its main button", async () => {
  const game = transport();
  const { result } = renderHook(() => useLiveSession(game.transport));
  await game.tell({ kind: "update", update: menu, canUndo: false });
  await act(async () => result.current.session!.dispatch({ type: "workspace", mode: "draft" }));
  await game.tell({ kind: "draft", script: game.asked[0], outcome: activation });
  await act(async () => result.current.session!.dispatch({ type: "toggleSystem", system: "23" }));
  expect(main(result.current)).toMatchObject({ label: "Activate system", disabled: false });

  // Another seat acted: the same script is played on the new game. Until it answers, the
  // screen is of the answer before, with the system staged and the main button as it was.
  await game.tell({ kind: "update", update: { ...menu }, canUndo: false });
  expect(game.asked).toHaveLength(2);
  expect(game.asked[1]).toBe(game.asked[0]);
  expect(main(result.current)).toMatchObject({ label: "Activate system", disabled: false });
  expect(result.current.session?.view.toolbar.draft?.applyHint).not.toBe(
    "The game checks the draft",
  );
  await game.tell({ kind: "draft", script: game.asked[1], outcome: activation });
  expect(main(result.current)).toMatchObject({ label: "Activate system", disabled: false });

  // A new question of the viewer starts with nothing staged.
  const asked = { ...menu, pending_choice: { ...menu.pending_choice!, nonce: "21" } };
  await game.tell({ kind: "update", update: asked, canUndo: false });
  await game.tell({ kind: "draft", script: game.asked[2], outcome: activation });
  expect(main(result.current)).toMatchObject({ label: "Activate system", disabled: true });
});

it("keeps what is staged in Live while another seat acts", async () => {
  const game = transport();
  const { result } = renderHook(() => useLiveSession(game.transport));
  await game.tell({ kind: "update", update: menu, canUndo: false });
  const staged = () => result.current.session!.view.action;
  await act(async () =>
    result.current.session!.dispatch({ type: "chooseOption", option: "tactical" }),
  );
  const chosen = staged();
  expect(main(result.current)?.disabled).toBeFalsy();

  await game.tell({ kind: "update", update: { ...menu }, canUndo: false });
  expect(staged()).toEqual(chosen);
  expect(main(result.current)?.disabled).toBeFalsy();
});

it("opens the same question again after the answer was refused", async () => {
  const game = transport();
  const { result } = renderHook(() => useLiveSession(game.transport));
  await game.tell({ kind: "update", update: menu, canUndo: false });
  await act(async () =>
    result.current.session!.dispatch({ type: "chooseOption", option: "strategic" }),
  );
  await act(async () => result.current.session!.dispatch({ type: "flow", action: "resolve" }));
  expect(game.made.submitChoice).toHaveBeenCalledTimes(1);
  const answered = main(result.current);

  // The update of another seat leaves the answer sent.
  await game.tell({ kind: "update", update: { ...menu }, canUndo: false });
  expect(main(result.current)).toEqual(answered);

  // The transport says that the answer was refused, and tells the same question again.
  await game.tell({ kind: "rejected", reason: "stale" });
  await game.tell({ kind: "update", update: { ...menu }, canUndo: false });
  expect(main(result.current)).not.toEqual(answered);
});
