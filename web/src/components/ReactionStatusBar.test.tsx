import { act } from "react";
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ReactionStatusBar } from "./ReactionStatusBar.tsx";
import { PendingChoiceDto } from "../protocol/types.ts";

describe("ReactionStatusBar", () => {
  it("renders reaction card options and handles click", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);

    const reactionChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "40",
      prompt: "Play Sabotage to cancel Action Card?",
      context: {
        subtype: "play_reaction_when_action_card_played",
      },
      options: [
        { id: "sabotage", label: "Sabotage", kind: "reaction" },
        { id: "decline", label: "Pass", kind: "decline" },
      ],
    };

    render(
      <ReactionStatusBar
        isOpen={true}
        choice={reactionChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
      />,
    );

    expect(screen.getByTestId("reaction-bar-prompt")).toHaveTextContent(
      "Play Sabotage to cancel Action Card?",
    );
    const playBtn = screen.getByTestId("play-reaction-btn-sabotage");
    expect(playBtn).toBeInTheDocument();

    await act(async () => {
      fireEvent.click(playBtn);
    });
    expect(onSubmit).toHaveBeenCalledWith("sabotage");
  });

  it("handles pass button click", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);

    const reactionChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "41",
      prompt: "Reaction opportunity",
      options: [
        { id: "sabotage", label: "Sabotage" },
        { id: "decline", label: "Pass", kind: "decline" },
      ],
    };

    render(
      <ReactionStatusBar
        isOpen={true}
        choice={reactionChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
      />,
    );

    const passBtn = screen.getByTestId("pass-reaction-btn");
    await act(async () => {
      fireEvent.click(passBtn);
    });
    expect(onSubmit).toHaveBeenCalledWith("decline");
  });

  it("triggers pass on Spacebar keypress", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);

    const reactionChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "42",
      prompt: "Reaction opportunity",
      options: [
        { id: "sabotage", label: "Sabotage" },
        { id: "decline", label: "Pass", kind: "decline" },
      ],
    };

    render(
      <ReactionStatusBar
        isOpen={true}
        choice={reactionChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
      />,
    );

    await act(async () => {
      fireEvent.keyDown(window, { key: " ", code: "Space" });
    });
    expect(onSubmit).toHaveBeenCalledWith("decline");
  });

  it("triggers reaction play on Enter keypress", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);

    const reactionChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "43",
      prompt: "Reaction opportunity",
      options: [
        { id: "sabotage", label: "Sabotage" },
        { id: "decline", label: "Pass", kind: "decline" },
      ],
    };

    render(
      <ReactionStatusBar
        isOpen={true}
        choice={reactionChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
      />,
    );

    await act(async () => {
      fireEvent.keyDown(window, { key: "Enter", code: "Enter" });
    });
    expect(onSubmit).toHaveBeenCalledWith("sabotage");
  });

  it("displays spectator notice for non-active viewer", () => {
    const onSubmit = vi.fn();

    const reactionChoice: PendingChoiceDto = {
      actor: "seat_2",
      nonce: "44",
      prompt: "Reaction opportunity",
      options: [{ id: "sabotage", label: "Sabotage" }],
    };

    render(
      <ReactionStatusBar
        isOpen={true}
        choice={reactionChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
      />,
    );

    expect(screen.getByTestId("spectator-reaction-notice")).toHaveTextContent(
      "Waiting for Unknown participant...",
    );
  });

  it("displays error badge when lastError is provided", () => {
    const onSubmit = vi.fn();

    const reactionChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "45",
      prompt: "Reaction opportunity",
      options: [{ id: "decline", label: "Pass" }],
    };

    render(
      <ReactionStatusBar
        isOpen={true}
        choice={reactionChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        lastError="Reaction window expired"
      />,
    );

    expect(screen.getByTestId("reaction-error-badge")).toHaveTextContent("Reaction window expired");
  });
});
