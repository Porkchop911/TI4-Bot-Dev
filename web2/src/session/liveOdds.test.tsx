import { act, renderHook } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import type { MovementView } from "../model";
import battleUpdate from "./fixtures/update-battle.json";
import type { Transport, TransportEvent } from "./transport";
import { useLiveSession } from "./useLiveSession";
import type { SessionUpdate, SpaceOddsRequest, SpaceOddsResponse } from "./wire";

// The odds of the space combat that the staged movement starts: the session asks the engine of
// the page for each new movement, and shows the last answer as pending until the next one.
//   cargo run -p ti4-wasm --example record -- late-game 8 1 3 tactical 32 > web2/src/session/fixtures/update-battle.json
const battle = battleUpdate as unknown as SessionUpdate;

const answer = (win: number): SpaceOddsResponse => ({
  simulations: 2000,
  attacker_win_rate: win,
  defender_win_rate: 1 - win,
  mutual_destruction_rate: 0,
  unresolved_rate: 0,
  average_rounds: 2,
  attacker_expected_survivors: { warsun: 1 },
  defender_expected_survivors: {},
  attacker_fielded: { warsun: 1 },
  defender_fielded: { cruiser: 2, destroyer: 1 },
});

function transport() {
  let listener: (event: TransportEvent) => void = () => {};
  /** The questions for the odds, each with the function that answers it. */
  const asked: { request: SpaceOddsRequest; answer: (response: SpaceOddsResponse) => void }[] = [];
  const made = {
    catalog: null,
    capabilities: {},
    subscribe: (to: typeof listener) => {
      listener = to;
      return () => {};
    },
    odds: vi.fn(
      (request: SpaceOddsRequest) =>
        new Promise<SpaceOddsResponse>((resolve) => asked.push({ request, answer: resolve })),
    ),
  };
  const tell = (event: TransportEvent) => act(async () => listener(event));
  return { transport: made as unknown as Transport, made, asked, tell };
}

it("asks the engine for the odds of each movement, and shows the last answer as pending", async () => {
  const game = transport();
  const { result } = renderHook(() => useLiveSession(game.transport));
  const session = () => result.current.session!;
  const odds = () => {
    const { action } = session().view;
    return action.kind === "tactical" ? (action.task.content as MovementView).odds : undefined;
  };
  const stage = (ship: string, moves = true) =>
    act(() => session().dispatch({ type: "setCount", key: `ship:${ship}`, value: Number(moves) }));
  await game.tell({ kind: "update", update: battle, canUndo: false });
  // Nothing moves in: no question, no odds.
  expect(odds()).toBeNull();
  expect(game.made.odds).not.toHaveBeenCalled();

  stage("01|1");
  expect(game.asked.map(({ request }) => request.attacker.units)).toEqual([{ warsun: 1 }]);
  expect(odds()).toMatchObject({ defender: "c", win: null, pending: true });
  await act(async () => game.asked[0].answer(answer(0.6)));
  expect(odds()).toMatchObject({ win: 60, draw: 0, loss: 40, pending: false });
  // The active system shows the same odds in its inspector.
  act(() => session().dispatch({ type: "inspectSystem", system: "32" }));
  expect(session().view.board.inspector?.arriving?.odds).toEqual(odds());

  // Another ship: the numbers of the last answer are pending until the new one.
  stage("01|2");
  expect(game.asked).toHaveLength(2);
  expect(odds()).toMatchObject({ win: 60, pending: true });
  await act(async () => game.asked[1].answer(answer(0.8)));
  expect(odds()).toMatchObject({ win: 80, pending: false });

  // An answer for a movement that changed since is not kept.
  stage("01|2", false);
  stage("01|0");
  expect(game.asked).toHaveLength(4);
  await act(async () => game.asked[2].answer(answer(0.1)));
  expect(odds()).toMatchObject({ win: 80, pending: true });
  await act(async () => game.asked[3].answer(answer(0.9)));
  expect(odds()).toMatchObject({ win: 90, pending: false });

  // A new choice starts with nothing staged, and with no odds kept.
  await game.tell({ kind: "update", update: battle, canUndo: false, undone: [] });
  expect(odds()).toBeNull();
  stage("01|1");
  expect(odds()).toMatchObject({ win: null, pending: true });
});
