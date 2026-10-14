import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { SystemActivationBar } from "./SystemActivationBar.tsx";
import { PendingChoiceDto, BoardView } from "../protocol/types.ts";

describe("SystemActivationBar Component", () => {
  const mockChoice: PendingChoiceDto = {
    nonce: "test-nonce-1",
    actor: "player1",
    prompt: "Choose a system to activate",
    options: [
      { id: "18", label: "Activate Mecatol Rex", kind: "activate", payload: { system: "18" } },
      { id: "24", label: "Activate Mehar Xull", kind: "activate", payload: { system: "24" } },
    ],
  };

  const mockBoardView: BoardView = {
    systems: {},
    map_tiles: [
      { system_id: "18", label: "Mecatol Rex", q: 0, r: 0 },
      { system_id: "24", label: "Mehar Xull", q: 1, r: 0 },
    ],
  };

  it("renders waiting message for non-actor viewer", () => {
    render(
      <SystemActivationBar choice={mockChoice} viewerSeat="other_player" onSubmit={vi.fn()} />,
    );

    expect(screen.getByText(/Waiting for player1 to activate a system/i)).toBeInTheDocument();
    expect(screen.queryByTestId("confirm-activation-btn")).not.toBeInTheDocument();
  });

  it("renders prompt instructions when actor has no system selected yet", () => {
    render(
      <SystemActivationBar
        choice={mockChoice}
        viewerSeat="player1"
        onSubmit={vi.fn()}
        boardView={mockBoardView}
      />,
    );

    expect(screen.getByText("Tactical Action")).toBeInTheDocument();
    expect(screen.getByText("Choose a system to activate")).toBeInTheDocument();
    expect(screen.getByText(/\(Click a highlighted system hex\)/i)).toBeInTheDocument();
    expect(screen.queryByTestId("confirm-activation-btn")).not.toBeInTheDocument();
  });

  it("renders confirmation row with system label and buttons when an option is selected", () => {
    const onSelectOption = vi.fn();
    const onSubmit = vi.fn();

    render(
      <SystemActivationBar
        choice={mockChoice}
        viewerSeat="player1"
        selectedOptionId="18"
        onSelectOption={onSelectOption}
        onSubmit={onSubmit}
        boardView={mockBoardView}
      />,
    );

    expect(screen.getByText(/Activate/)).toBeInTheDocument();
    expect(screen.getByText("Mecatol Rex")).toBeInTheDocument();
    expect(screen.getByText(/#18/)).toBeInTheDocument();
    expect(screen.getByTestId("confirm-activation-btn")).toBeInTheDocument();
    expect(screen.getByTestId("cancel-activation-btn")).toBeInTheDocument();

    // Cancel click
    fireEvent.click(screen.getByTestId("cancel-activation-btn"));
    expect(onSelectOption).toHaveBeenCalledWith("");
  });

  it("submits choice when confirm button is clicked", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);

    render(
      <SystemActivationBar
        choice={mockChoice}
        viewerSeat="player1"
        selectedOptionId="24"
        onSubmit={onSubmit}
        boardView={mockBoardView}
      />,
    );

    expect(screen.getByText("Mehar Xull")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("confirm-activation-btn"));

    await waitFor(() => {
      expect(onSubmit).toHaveBeenCalledWith("24");
    });
  });

  it("displays error when submission rejects", async () => {
    const onSubmit = vi.fn().mockRejectedValue(new Error("Fleet supply exceeded"));

    render(
      <SystemActivationBar
        choice={mockChoice}
        viewerSeat="player1"
        selectedOptionId="18"
        onSubmit={onSubmit}
        boardView={mockBoardView}
      />,
    );

    fireEvent.click(screen.getByTestId("confirm-activation-btn"));

    await waitFor(() => {
      expect(screen.getByTestId("activation-error")).toHaveTextContent("Fleet supply exceeded");
    });
  });

  it("informs the player when inspecting a system blocked by their command token", () => {
    const boardWithTokens: BoardView = {
      systems: {
        "19": { system_id: "19", command_tokens: ["player1"], planets: {}, units: [] },
      },
      map_tiles: [{ system_id: "19", label: "Wellon", q: 1, r: -1 }],
    };

    render(
      <SystemActivationBar
        choice={mockChoice}
        viewerSeat="player1"
        selectedSystemId="19"
        onSubmit={vi.fn()}
        boardView={boardWithTokens}
      />,
    );

    expect(screen.getByText("Activated / Blocked")).toBeInTheDocument();
    expect(screen.getByText(/already contains your command token/i)).toBeInTheDocument();
  });

  it("informs the player when inspecting a system that is not a legal target", () => {
    const boardWithNeutral: BoardView = {
      systems: {
        "30": { system_id: "30", command_tokens: [], planets: {}, units: [] },
      },
      map_tiles: [{ system_id: "30", label: "Centauri / Gral", q: -2, r: 0 }],
    };

    render(
      <SystemActivationBar
        choice={mockChoice}
        viewerSeat="player1"
        selectedSystemId="30"
        onSubmit={vi.fn()}
        boardView={boardWithNeutral}
      />,
    );

    expect(screen.getByText("Not Targetable")).toBeInTheDocument();
    expect(screen.getByText(/cannot be activated in this action/i)).toBeInTheDocument();
  });

  it("does not render confirm activation row when selectedSystemId does not match selectedOptionId", () => {
    const boardWithTokens: BoardView = {
      systems: {
        "19": { system_id: "19", command_tokens: ["player1"], planets: {}, units: [] },
      },
      map_tiles: [
        { system_id: "19", label: "Wellon", q: 1, r: -1 },
        { system_id: "24", label: "Mehar Xull", q: 1, r: 0 },
      ],
    };

    render(
      <SystemActivationBar
        choice={mockChoice}
        viewerSeat="player1"
        selectedOptionId="24"
        selectedSystemId="19"
        onSubmit={vi.fn()}
        boardView={boardWithTokens}
      />,
    );

    expect(screen.queryByTestId("confirm-activation-btn")).not.toBeInTheDocument();
    expect(screen.queryByText(/Activate Mehar Xull/i)).not.toBeInTheDocument();
    expect(screen.getByText("Activated / Blocked")).toBeInTheDocument();
    expect(screen.getByText(/already contains your command token/i)).toBeInTheDocument();
  });
});
