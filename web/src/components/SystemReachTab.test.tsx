import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { Board } from "./Board.tsx";
import type { BoardView } from "../protocol/types.ts";
import type { ReachReply } from "../protocol/client.ts";
import { ReachUnsupportedError } from "../protocol/client.ts";

const board: BoardView = {
  systems: {
    "18": { system_id: "18", planets: {}, units: [], command_tokens: [] },
    "27": {
      system_id: "27",
      planets: {},
      units: [{ unit_type: "cruiser", owner: "p1", damaged: false }],
      command_tokens: [],
    },
    "41": {
      system_id: "41",
      planets: {},
      units: [{ unit_type: "dreadnought", owner: "p2", damaged: false }],
      command_tokens: [],
    },
  },
  map_tiles: [
    { system_id: "18", label: "Mecatol Rex", q: 0, r: 0, planets: [] },
    { system_id: "27", label: "Lodor", q: 1, r: -1, planets: [] },
    { system_id: "41", label: "Void", q: -1, r: 1, planets: [] },
  ],
};

const ship = (type_id: string, count: number) => ({
  type_id,
  count,
  damaged: 0,
  gravity_drive: 0,
  ionian: 0,
});

const reply = (origins: Extract<ReachReply, { kind: "reach" }>["origins"]): ReachReply => ({
  kind: "reach",
  systemId: "18",
  asOfVersion: 3,
  origins,
});

async function openReach(queryReach: (id: string) => Promise<ReachReply>, viewerSeat: string | null = "p1") {
  render(
    <Board board={board} seatingOrder={["p1", "p2"]} viewerSeat={viewerSeat} queryReach={queryReach} />,
  );
  fireEvent.click(screen.getByTestId("system-hex-18"));
  await act(async () => {
    fireEvent.click(screen.getByTestId("inspector-tab-reach"));
  });
}

describe("Who can go here", () => {
  it("offers no tab without the query", () => {
    render(<Board board={board} seatingOrder={["p1", "p2"]} viewerSeat="p1" />);
    fireEvent.click(screen.getByTestId("system-hex-18"));
    expect(screen.queryByTestId("inspector-tab-reach")).toBeNull();
  });

  it("asks only once the tab is open, lists own and enemy ships and draws coloured arrows", async () => {
    const queryReach = vi.fn(async () =>
      reply([
        { owner: "p1", origin: "27", ships: [ship("cruiser", 2)] },
        { owner: "p2", origin: "41", ships: [ship("dreadnought", 1)] },
      ]),
    );
    render(
      <Board board={board} seatingOrder={["p1", "p2"]} viewerSeat="p1" queryReach={queryReach} />,
    );
    fireEvent.click(screen.getByTestId("system-hex-18"));
    expect(queryReach).not.toHaveBeenCalled();
    await act(async () => {
      fireEvent.click(screen.getByTestId("inspector-tab-reach"));
    });
    await waitFor(() => expect(screen.getByTestId("reach-own")).toBeInTheDocument());
    expect(queryReach).toHaveBeenCalledWith("18");
    expect(screen.getByTestId("reach-assumption")).toHaveTextContent(/activated now/);
    expect(screen.getByTestId("reach-own")).toHaveTextContent("2 × Cruisers");
    expect(screen.getByTestId("reach-enemy")).toHaveTextContent("1 × Dreadnought");
    expect(screen.getAllByTestId("reach-vector-own")).toHaveLength(1);
    expect(screen.getAllByTestId("reach-vector-enemy")).toHaveLength(1);

    // Picking an entry fades the other arrow.
    const enemyEntry = screen.getAllByTestId("reach-entry").find((e) => e.dataset.owner === "p2")!;
    fireEvent.click(enemyEntry);
    expect(screen.getByTestId("reach-vector-own").style.opacity).toBe("0.25");
    expect(screen.getByTestId("reach-vector-enemy").style.opacity).toBe("1");

    // Back to Details removes the arrows.
    fireEvent.click(screen.getByTestId("inspector-tab-details"));
    expect(screen.queryByTestId("reach-vector-own")).toBeNull();
  });

  it("says so when nothing can move there", async () => {
    await openReach(async () => reply([]));
    await waitFor(() => expect(screen.getByTestId("reach-empty")).toBeInTheDocument());
    expect(screen.queryByTestId("reach-vector-own")).toBeNull();
  });

  it("explains an older server that cannot answer", async () => {
    await openReach(async () => {
      throw new ReachUnsupportedError();
    });
    await waitFor(() => expect(screen.getByTestId("reach-unsupported")).toBeInTheDocument());
  });

  it("shows every ship as other players' for a spectator", async () => {
    await openReach(
      async () => reply([{ owner: "p2", origin: "41", ships: [ship("dreadnought", 1)] }]),
      null,
    );
    await waitFor(() => expect(screen.getByTestId("reach-enemy")).toBeInTheDocument());
    expect(screen.queryByTestId("reach-own")).toBeNull();
  });
});
