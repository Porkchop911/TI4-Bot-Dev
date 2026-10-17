import { describe, expect, it, vi } from "vitest";
import { render, screen } from "@testing-library/react";
import { ChoiceRendererDispatcher } from "./GameShell.tsx";

// Nightly 2026-10-14 #13 (o2-186-0210): after the engine ended the game the read-only turn bar
// still said "Waiting for Bot 1" with every button "Not your turn".
describe("the turn bar after the game ended", () => {
  const props = {
    choice: null,
    viewerSeat: "p1",
    players: {
      p1: { id: "p1", faction: "sol", passed: false, strategy_cards: ["pok2diplomacy"], exhausted_strategy_cards: [] },
    } as never,
    onSubmit: vi.fn().mockResolvedValue(undefined),
    isMinimized: false,
    onMinimizedChange: () => undefined,
  };

  it("shows the read-only bar while another seat plays its turn", () => {
    render(<ChoiceRendererDispatcher {...props} turn={{ phase: "action", activePlayer: "p2" }} />);
    expect(screen.queryByTestId("turn-action-bar")).not.toBeNull();
  });

  it("shows no bar once the game is over", () => {
    render(<ChoiceRendererDispatcher {...props} turn={{ phase: "action", activePlayer: "p2", over: true }} />);
    expect(screen.queryByTestId("turn-action-bar")).toBeNull();
  });
});
