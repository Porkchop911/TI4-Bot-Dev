import React, { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { MobileTopMenu, menuAttention } from "./MobileTopMenu.tsx";
import { TurnStatusBar, shortStatusText } from "./TurnStatusBar.tsx";
import { Board } from "./Board.tsx";
import { MobileMenuSlotContext } from "../presentation/MobileMenuSlot.tsx";
import type { BoardView } from "../protocol/types.ts";

const original = window.matchMedia;
function mockPhone(matches: boolean) {
  window.matchMedia = ((query: string) => ({
    matches,
    media: query,
    addEventListener: () => undefined,
    removeEventListener: () => undefined,
  })) as unknown as typeof window.matchMedia;
}
afterEach(() => {
  window.matchMedia = original;
});

describe("menuAttention", () => {
  it("ranks the offline link, then a decision for you, then your turn", () => {
    expect(menuAttention({ connected: false, yourDecision: true, yourTurn: true })).toBe("offline");
    expect(menuAttention({ connected: true, yourDecision: true, yourTurn: true })).toBe("decision");
    expect(menuAttention({ connected: true, yourDecision: false, yourTurn: true })).toBe("turn");
    expect(menuAttention({ connected: true, yourDecision: false, yourTurn: false })).toBeNull();
  });
});

describe("shortStatusText", () => {
  const label = (id: string) => (id === "a" ? "Alice" : id);
  it("names who is up in a few words and leaves the stage to the menu", () => {
    expect(shortStatusText({ kind: "waiting_for_decision", seat: "a", phase: "action", round: 1, stage: "activate_system" }, "a", label)).toBe("YOUR TURN");
    expect(shortStatusText({ kind: "waiting_for_decision", seat: "a", phase: "action", round: 1, stage: "x" }, "b", label)).toBe("Waiting: Alice");
    expect(shortStatusText({ kind: "active_turn", player: "a", phase: "action", round: 1 }, "b", label)).toBe("Alice's turn");
    expect(shortStatusText({ kind: "waiting_for_reactions", phase: "action", round: 1 }, "b", label)).toBe("Waiting for reactions");
    expect(shortStatusText({ kind: "game_over", winner: "a" } as never, "b", label)).toBe("Game Over! Winner: Alice");
  });
});

describe("TurnStatusBar compact", () => {
  it("keeps the test ids the harness reads, in one row", () => {
    render(
      <TurnStatusBar
        status={{ kind: "waiting_for_decision", seat: "p1", phase: "action", round: 2, stage: "activate_system" }}
        view={{ round: 2, phase: "action", speaker: "p1" } as never}
        gameVersion={12}
        connectionStatus="connecting"
        userSeat="p1"
        compact
        trailing={<span data-testid="trailing" />}
      />,
    );
    expect(screen.getByTestId("turn-status-bar")).toHaveAttribute("data-compact", "true");
    expect(screen.getByTestId("connection-indicator")).toHaveAttribute("data-status", "connecting");
    expect(screen.getByTestId("game-version")).toHaveTextContent("v12");
    expect(screen.getByTestId("turn-status-banner")).toHaveTextContent("YOUR TURN");
    expect(screen.getByTestId("turn-status-bar")).toHaveTextContent("R2 · action");
    expect(screen.getByTestId("trailing")).toBeInTheDocument();
  });
});

describe("MobileTopMenu", () => {
  const setup = (attention: Parameters<typeof menuAttention>[0] | null = null) => {
    const onTech = vi.fn();
    const onObj = vi.fn();
    render(
      <MobileTopMenu
        attention={attention ? menuAttention(attention) : null}
        status={<span>status text</span>}
        onOpenTechnologies={onTech}
        onOpenObjectives={onObj}
      />,
    );
    return { onTech, onObj };
  };

  it("is closed until the button is pressed, and Escape closes it", () => {
    setup();
    expect(screen.queryByTestId("top-menu-sheet")).toBeNull();
    fireEvent.click(screen.getByTestId("top-menu-button"));
    expect(screen.getByTestId("top-menu-sheet")).toBeInTheDocument();
    expect(screen.getByTestId("top-menu-status")).toHaveTextContent("status text");
    fireEvent.keyDown(screen.getByTestId("top-menu-sheet"), { key: "Escape" });
    expect(screen.queryByTestId("top-menu-sheet")).toBeNull();
  });

  it("Technologies and Objectives close the sheet and open their overlay", () => {
    const { onTech, onObj } = setup();
    fireEvent.click(screen.getByTestId("top-menu-button"));
    fireEvent.click(screen.getByTestId("technology-modal-button"));
    expect(onTech).toHaveBeenCalledTimes(1);
    expect(screen.queryByTestId("top-menu-sheet")).toBeNull();
    fireEvent.click(screen.getByTestId("top-menu-button"));
    fireEvent.click(screen.getByTestId("objectives-modal-button"));
    expect(onObj).toHaveBeenCalledTimes(1);
  });

  it("shows a dot on the button only when something needs attention", () => {
    const { unmount } = render(
      <MobileTopMenu attention={null} status={null} onOpenTechnologies={vi.fn()} onOpenObjectives={vi.fn()} />,
    );
    expect(screen.queryByTestId("top-menu-dot")).toBeNull();
    unmount();
    render(
      <MobileTopMenu attention="decision" status={null} onOpenTechnologies={vi.fn()} onOpenObjectives={vi.fn()} />,
    );
    expect(screen.getByTestId("top-menu-dot")).toHaveAttribute("data-kind", "decision");
    expect(screen.getByTestId("top-menu-button")).toHaveAccessibleName(/decision is waiting/);
  });
});

const board: BoardView = {
  systems: [{ id: "18", label: "Mecatol Rex", position: { x: 0, y: 0 }, planets: [], units: [], tokens: [], wormholes: [], anomalies: [] }],
  adjacency: [],
} as unknown as BoardView;

describe("Board controls on a phone", () => {
  const Host: React.FC = () => {
    const [slot, setSlot] = useState<HTMLElement | null>(null);
    return (
      <MobileMenuSlotContext.Provider value={{ slot, setSlot }}>
        <MobileTopMenu attention={null} status={null} onOpenTechnologies={vi.fn()} onOpenObjectives={vi.fn()} />
        <Board board={board} seatingOrder={["p1", "p2"]} />
      </MobileMenuSlotContext.Provider>
    );
  };

  beforeEach(() => mockPhone(true));

  it("keeps zoom, view and who's who out of the board row and puts them in the open sheet", () => {
    render(<Host />);
    expect(screen.getByTestId("board-chrome")).toHaveClass("board-chrome--compact");
    expect(screen.queryByTestId("map-overlay-toolbar")).toBeNull();
    expect(screen.queryByTitle("Zoom In")).toBeNull();
    fireEvent.click(screen.getByTestId("top-menu-button"));
    expect(screen.getByTestId("top-menu-slot")).toContainElement(screen.getByTestId("map-overlay-toolbar"));
    expect(screen.getByTestId("top-menu-slot")).toContainElement(screen.getByTitle("Zoom In"));
    expect(screen.getByTestId("top-menu-slot")).toHaveTextContent("Who’s who");
  });

  it("zooms from the sheet", () => {
    render(<Host />);
    const transform = () => screen.getByTestId("ti4-board-svg").querySelector("g")!.getAttribute("transform");
    const start = transform();
    fireEvent.click(screen.getByTestId("top-menu-button"));
    fireEvent.click(screen.getByTitle("Zoom In"));
    expect(transform()).not.toBe(start);
  });

  it("leaves the toolbar row alone without the menu (desktop, or no provider)", () => {
    render(<Board board={board} seatingOrder={["p1", "p2"]} />);
    expect(screen.getByTestId("board-chrome")).not.toHaveClass("board-chrome--compact");
    expect(screen.getByTestId("map-overlay-toolbar")).toBeInTheDocument();
    expect(screen.getByTitle("Zoom In")).toBeInTheDocument();
  });
});
