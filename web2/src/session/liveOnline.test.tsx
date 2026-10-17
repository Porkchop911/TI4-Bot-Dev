import { act, renderHook } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import menuUpdate from "../../../fixtures/protocol/update-menu.json";
import type { Transport, TransportEvent } from "./transport";
import { useLiveSession } from "./useLiveSession";
import type { SessionUpdate } from "./wire";

// What only a game on the server tells (`wsTransport.ts`): that it cannot be reached, and that
// it did not take an answer. The page shows both; it does not know where the game runs.
const menu = menuUpdate as unknown as SessionUpdate;

function transport() {
  let listener: (event: TransportEvent) => void = () => {};
  const made = {
    catalog: null,
    capabilities: {},
    subscribe: (to: typeof listener) => {
      listener = to;
      return () => {};
    },
    submitChoice: vi.fn(),
  };
  const tell = (event: TransportEvent) => act(async () => listener(event));
  return { made, transport: made as unknown as Transport, tell };
}

it("says in the toolbar that the game is offline, and sends nothing until the next update", async () => {
  const game = transport();
  const { result } = renderHook(() => useLiveSession(game.transport));
  const view = () => result.current.session!.view;
  const main = () => {
    const { action } = view();
    const footer = action.kind === "flow" ? action.footer : action.task.footer;
    return footer.actions.find((button) => button.tone === "primary");
  };
  await game.tell({ kind: "update", update: menu, canUndo: false });
  expect(view().toolbar.status).toBe("Your decision");
  // A choice is staged: the main button would send it.
  act(() => result.current.session?.dispatch({ type: "chooseOption", option: "tactical" }));
  expect(main()?.disabled).toBeFalsy();

  await game.tell({ kind: "offline" });
  // One signal: the status of the toolbar. The game stays in view, and it is no error.
  expect(view().toolbar.status).toBe("Offline · reconnecting");
  expect(result.current.error).toBeNull();
  expect(view().players.rows.length).toBeGreaterThan(0);
  expect(JSON.stringify(view().action)).toContain("The game cannot be reached.");
  // No choice is open while the game cannot be reached: the main button waits, and Enter sends nothing.
  expect(main()?.disabled ?? true).toBe(true);
  act(() => result.current.session?.dispatch({ type: "flow", action: "resolve" }));
  expect(game.made.submitChoice).not.toHaveBeenCalled();

  // The first update after it is the game as it is now.
  await game.tell({ kind: "update", update: menu, canUndo: false });
  expect(view().toolbar.status).toBe("Your decision");
});

it("says why the game did not take an answer", async () => {
  const game = transport();
  const { result } = renderHook(() => useLiveSession(game.transport));
  await game.tell({ kind: "update", update: menu, canUndo: false });
  await game.tell({ kind: "rejected", reason: "The game went on before your answer arrived." });
  expect(result.current.session?.view.toast?.text).toBe(
    "The game went on before your answer arrived.",
  );
  expect(result.current.error).toBeNull();
});

it("says who the game waits for when only that changed, and keeps what is staged", async () => {
  const game = transport();
  const { result } = renderHook(() => useLiveSession(game.transport));
  const view = () => result.current.session!.view;
  await game.tell({ kind: "update", update: menu, canUndo: false });
  act(() => result.current.session?.dispatch({ type: "chooseOption", option: "tactical" }));
  const staged = JSON.stringify(view().action);
  const other = menu.view.players.find(
    (player) => menu.viewer.role === "player" && player.id !== menu.viewer.seat,
  )!;
  await game.tell({
    kind: "status",
    status: { kind: "waiting_for_decision", seat: other.id, phase: "action", round: 1, stage: "" },
  });
  // The seat is still asked itself, with its choice staged.
  expect(view().toolbar.status).toBe("Your decision");
  expect(JSON.stringify(view().action)).toBe(staged);

  // A seat that is not asked reads who is.
  await game.tell({
    kind: "update",
    update: { ...menu, pending_choice: undefined, tactical: undefined },
    canUndo: false,
  });
  await game.tell({
    kind: "status",
    status: { kind: "waiting_for_decision", seat: other.id, phase: "action", round: 1, stage: "" },
  });
  expect(view().toolbar.status).toMatch(/^Waiting for (?!Seat )\S/);
  await game.tell({
    kind: "status",
    status: { kind: "waiting_for_reactions", phase: "action", round: 1 },
  });
  expect(view().toolbar.status).toBe("Waiting for reactions");
});
