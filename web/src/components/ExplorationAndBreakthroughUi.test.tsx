import { describe, it, expect } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { renderHook } from "@testing-library/react";
import { Board } from "./Board.tsx";
import { PlayerSheet } from "./PlayerSheet.tsx";
import { SystemInspector } from "./SystemInspector.tsx";
import { DecisionHeader } from "./DecisionHeader.tsx";
import { FactionInfoCard } from "./UnitInfo.tsx";
import { PublicStateProvider } from "../presentation/PublicStateContext.tsx";
import { describeFaction } from "../presentation/factionInfo.ts";
import { useCornerToasts } from "../hooks/useCornerToasts.ts";
import type { BoardView, PlayerView } from "../protocol/types.ts";
import type { SelectedSystemDetails } from "../presentation/boardPresentation.ts";

const board: BoardView = {
  systems: {
    "34": {
      system_id: "34",
      coordinate: "<1, 0, -1>",
      tile_type: "normal",
      planets: {
        abyz: { planet_id: "abyz", controlled_by: "p1", exhausted: false, attachments: ["dysonsphere"] },
      },
      units: [],
      command_tokens: [],
    },
    "50": { system_id: "50", coordinate: "<2, 0, -2>", tile_type: "normal", planets: {}, units: [], command_tokens: [] },
    "51": { system_id: "51", coordinate: "<3, 0, -3>", tile_type: "normal", planets: {}, units: [], command_tokens: [] },
  },
};

const state = {
  frontier_tokens: ["51"],
  exploration_log: [
    { round: 1, player: "p1", deck: "INDUSTRIAL", card: "aw2", planet: "abyz", outcome: "resolved", result: "gained 2 commodities" },
    { round: 1, player: "p1", deck: "FRONTIER", card: "ed2", planet: null, system: "50", outcome: "resolved" },
  ],
  players: [
    { id: "p1", breakthrough: "solbt" },
    { id: "p2", breakthrough: null },
  ],
};

const player = (id: string, faction: string): PlayerView =>
  ({
    id,
    faction,
    victory_points: 0,
    trade_goods: 0,
    commodities: 0,
    tactic_tokens: 3,
    fleet_tokens: 3,
    strategic_tokens: 2,
    passed: false,
    strategy_cards: [],
    exhausted_strategy_cards: [],
    technologies: [],
    exhausted_technologies: [],
    relics: [],
    exhausted_relics: [],
    action_cards_count: 0,
    secret_objectives_count: 0,
    leaders: {},
  }) as PlayerView;

describe("map markers for exploration", () => {
  it("marks attachments, explored systems and a waiting frontier token on their tiles", () => {
    render(
      <PublicStateProvider state={state}>
        <Board board={board} seatingOrder={["p1", "p2"]} />
      </PublicStateProvider>,
    );
    expect(screen.getByTestId("exploration-mark-attachments-34")).toHaveTextContent("+");
    expect(within(screen.getByTestId("exploration-mark-explored-34")).getByText("1")).toBeInTheDocument();
    expect(screen.getByTestId("exploration-mark-explored-50")).toBeInTheDocument();
    expect(screen.getByTestId("exploration-mark-frontier-51")).toHaveTextContent("?");
    expect(screen.queryByTestId("exploration-mark-frontier-50")).toBeNull();
  });
  it("draws nothing without exploration facts", () => {
    render(<Board board={board} seatingOrder={["p1"]} />);
    // The attachment comes from the board itself; nothing explored or waiting is invented.
    expect(screen.queryByTestId("exploration-mark-explored-34")).toBeNull();
    expect(screen.queryByTestId("exploration-mark-frontier-51")).toBeNull();
  });
});

describe("system overview exploration entries", () => {
  const system = (id: string, planets: SelectedSystemDetails["planets"]): SelectedSystemDetails => ({
    systemId: id,
    label: id,
    anomalies: [],
    wormholes: [],
    specialArea: null,
    isActiveSystem: false,
    planets,
    spaceUnits: [],
    planetUnits: {},
    commandTokens: [],
    availableActions: [],
  });
  it("lists what was explored in the system with the card and the gain", () => {
    render(
      <PublicStateProvider state={state}>
        <SystemInspector
          system={system("34", [
            {
              id: "abyz",
              label: "Abyz",
              resources: 3,
              influence: 0,
              traits: ["industrial"],
              techSpecialties: [],
              legendary: false,
              controlledBy: null,
              controllerColor: "#fff",
              exhausted: false,
              attachments: ["dysonsphere"],
              isCandidateTarget: false,
              isContextSubject: false,
              associatedOptionIds: [],
            },
          ])}
          onClose={() => undefined}
        />
      </PublicStateProvider>,
    );
    const entry = screen.getByTestId("inspector-exploration-entry");
    expect(entry).toHaveTextContent("Abandoned Warehouses");
    expect(entry).toHaveTextContent("gained 2 commodities");
    expect(screen.getByTestId("system-inspector")).toHaveTextContent("Attachments: Dyson Sphere");
  });
  it("tells that a frontier token still waits", () => {
    render(
      <PublicStateProvider state={state}>
        <SystemInspector system={system("51", [])} onClose={() => undefined} />
      </PublicStateProvider>,
    );
    expect(screen.getByTestId("inspector-frontier-token")).toBeInTheDocument();
  });
});

describe("breakthrough on the sheet, the faction card and decisions", () => {
  it("shows the unlocked card in full and a locked one by name", () => {
    render(
      <PublicStateProvider state={state}>
        <PlayerSheet players={[player("p1", "sol"), player("p2", "letnev")]} userSeat="p1" />
      </PublicStateProvider>,
    );
    const rows = screen.getAllByTestId("player-breakthrough");
    expect(rows).toHaveLength(2);
    expect(rows[0]).toHaveAttribute("data-unlocked", "true");
    expect(within(rows[0]).getByTestId("player-breakthrough-name")).toHaveTextContent("Bellum Gloriosum");
    expect(within(rows[0]).getByTestId("player-breakthrough-text").textContent?.length).toBeGreaterThan(20);
    expect(rows[1]).toHaveAttribute("data-unlocked", "false");
    expect(within(rows[1]).getByTestId("player-breakthrough-state")).toHaveTextContent("locked");
    expect(within(rows[1]).queryByTestId("player-breakthrough-text")).toBeNull();
  });
  it("is a section of the faction card with its lock state", () => {
    const info = describeFaction("sol")!;
    render(<FactionInfoCard faction={info} breakthroughHeld={false} />);
    expect(screen.getByTestId("faction-breakthrough")).toHaveTextContent("Bellum Gloriosum");
    expect(screen.getByTestId("faction-breakthrough")).toHaveTextContent("locked");
  });
  it("prints the card on a decision a breakthrough raised", () => {
    render(
      <DecisionHeader
        title="Deepgloom Executable: allow Scheming"
        onMinimize={() => undefined}
        choice={{ context: { subtype: "yssaril_scheming", source: { Content: "yssarilbt" } } } as never}
      />,
    );
    const card = screen.getByTestId("decision-breakthrough");
    expect(card).toHaveTextContent("Deepgloom Executable");
    expect(card.textContent?.length).toBeGreaterThan(40);
  });
});

describe("breakthrough toast", () => {
  it("toasts a newly unlocked breakthrough for every seat, not the one held at load", () => {
    type Props = Parameters<typeof useCornerToasts>[0];
    const base: Props = { events: [], viewerSeat: "p1", ready: true };
    const { result, rerender } = renderHook((p: Props) => useCornerToasts(p), {
      initialProps: { ...base, breakthroughs: { p1: null, p2: "letnevbt" } as Record<string, string | null> } as Props,
    });
    expect(result.current.notifications).toHaveLength(0);
    rerender({ ...base, breakthroughs: { p1: "solbt", p2: "letnevbt" } });
    expect(result.current.notifications).toHaveLength(1);
    expect(result.current.notifications[0].text).toContain("Bellum Gloriosum");
  });
});
