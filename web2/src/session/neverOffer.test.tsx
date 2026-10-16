import { act, renderHook } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import menuUpdate from "./fixtures/update-menu.json";
import type { Transport, TransportEvent } from "./transport";
import { useLiveSession } from "./useLiveSession";
import type { SessionUpdate } from "./wire";

// The Never cards are applied where the game runs (`ti4_engine::reaction_modes`; the test of a
// local game is in `wasmTransport.test.ts`). The page says what the transport tells, and asks
// the transport for a change. Seat a holds `scuttle` in this update of the real engine.
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
    setReactionMode: vi.fn(),
    undo: vi.fn(),
  };
  const tell = (event: TransportEvent) => act(async () => listener(event));
  return { made, transport: made as unknown as Transport, tell };
}

it("says that a window was passed for a card that is never offered", async () => {
  const game = transport();
  const { result } = renderHook(() => useLiveSession(game.transport));
  await game.tell({ kind: "update", update: menu, canUndo: true });
  await game.tell({ kind: "passed", cards: ["Sabotage", "Scuttle"] });
  expect(result.current.session?.view.toast?.text).toBe(
    "Sabotage, Scuttle: not offered now. The window was passed.",
  );
  // The page answers nothing and takes nothing back: the transport did.
  expect(game.made.submitChoice).not.toHaveBeenCalled();
  expect(game.made.undo).not.toHaveBeenCalled();
});

it("shows the cards that the transport never offers, and asks it for a change", async () => {
  const game = transport();
  const { result } = renderHook(() => useLiveSession(game.transport));
  await game.tell({ kind: "modes", modes: {} });
  await game.tell({ kind: "update", update: menu, canUndo: false });
  const scuttle = () =>
    result.current.session?.view.reference.actionCards?.hand.find((card) => card.name === name);
  const name = "Scuttle";
  expect(scuttle()?.offer?.mode).toBe("always");
  act(() => result.current.session?.dispatch({ type: "setOffer", key: name, name, mode: "never" }));
  expect(game.made.setReactionMode).toHaveBeenCalledWith(name, "never");
  expect(result.current.session?.view.toast?.text).toBe(
    `${name}: never offered. Its windows are passed and you are told.`,
  );
  // The setting is a view of the transport: it is shown when the transport says so.
  expect(scuttle()?.offer?.mode).toBe("always");
  await game.tell({ kind: "modes", modes: { [name]: "never" } });
  expect(scuttle()?.offer?.mode).toBe("never");
  act(() =>
    result.current.session?.dispatch({ type: "setOffer", key: name, name, mode: "ownturn" }),
  );
  expect(game.made.setReactionMode).toHaveBeenLastCalledWith(name, "ownturn");
  expect(result.current.session?.view.toast?.text).toBe(
    `${name}: offered on your turns only. Its other windows are passed and you are told.`,
  );
  act(() =>
    result.current.session?.dispatch({ type: "setOffer", key: name, name, mode: "always" }),
  );
  expect(game.made.setReactionMode).toHaveBeenLastCalledWith(name, "always");
});

it("keeps a faked card in the page: the transport is not told", async () => {
  const game = transport();
  const { result } = renderHook(() => useLiveSession(game.transport));
  await game.tell({ kind: "update", update: menu, canUndo: false });
  act(() =>
    result.current.session?.dispatch({ type: "setCardFake", card: "Sabotage", fake: true }),
  );
  const cards = result.current.session?.view.reference.actionCards;
  expect(cards?.faked.map((card) => card.name)).toEqual(["Sabotage"]);
  expect(game.made.setReactionMode).not.toHaveBeenCalled();
});
