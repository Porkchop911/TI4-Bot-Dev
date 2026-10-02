import { act } from "react";
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { AgendaBallotModal } from "./AgendaBallotModal.tsx";
import { PendingChoiceDto } from "../protocol/types.ts";

describe("AgendaBallotModal", () => {
  it("renders cast_vote stage with live tallies and handles outcome selection and abstain", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    const castChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "30",
      prompt: "vote for which outcome",
      context: {
        subtype: "cast_vote",
      },
      options: [
        { id: "FOR", label: "FOR", kind: "vote", payload: { current_votes: 12 } },
        { id: "AGAINST", label: "AGAINST", kind: "vote", payload: { current_votes: 8 } },
        { id: "decline", label: "Abstain", kind: "decline" },
      ],
    };

    render(
      <AgendaBallotModal
        isOpen={true}
        choice={castChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    expect(screen.getByTestId("agenda-ballot-title")).toHaveTextContent("Choose a voting outcome");
    expect(screen.getByTestId("outcome-tally-FOR")).toHaveTextContent("12 votes cast");
    expect(screen.getByTestId("outcome-tally-AGAINST")).toHaveTextContent("8 votes cast");

    // Click FOR
    const forBtn = screen.getByTestId("vote-outcome-opt-FOR");
    await act(async () => {
      fireEvent.click(forBtn);
    });
    expect(onSubmit).toHaveBeenCalledWith("FOR");

    // Test abstain
    const abstainBtn = screen.getByTestId("abstain-vote-btn");
    await act(async () => {
      fireEvent.click(abstainBtn);
    });
    expect(onSubmit).toHaveBeenCalledWith("decline");
  });

  it("renders vote_exhaust_planet stage with basket and pipeline execution", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    const exhaustChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "31",
      prompt: "exhaust a planet to vote FOR",
      context: {
        subtype: "vote_exhaust_planet",
      },
      options: [
        { id: "Mecatol Rex", label: "exhaust Mecatol Rex for 6 votes", kind: "vote_planet" },
        { id: "Jord", label: "exhaust Jord for 2 votes", kind: "vote_planet" },
        { id: "decline", label: "Done", kind: "decline" },
      ],
    };

    render(
      <AgendaBallotModal
        isOpen={true}
        choice={exhaustChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    expect(screen.getByTestId("agenda-ballot-title")).toHaveTextContent("Spend influence to vote");
    expect(screen.getByTestId("staged-votes-counter")).toHaveTextContent("+0 Votes");

    const commitBtn = screen.getByTestId("commit-planet-votes-btn");
    expect(commitBtn).toBeDisabled();

    // Stage Mecatol Rex
    const mecatolCard = screen.getByTestId("planet-card-Mecatol Rex");
    fireEvent.click(mecatolCard);
    expect(screen.getByTestId("staged-votes-counter")).toHaveTextContent("+6 Votes");
    expect(commitBtn).not.toBeDisabled();

    // Stage Jord
    const jordCard = screen.getByTestId("planet-card-Jord");
    fireEvent.click(jordCard);
    expect(screen.getByTestId("staged-votes-counter")).toHaveTextContent("+8 Votes");

    // Commit planet votes triggers pipeline
    await act(async () => {
      fireEvent.click(commitBtn);
    });
    expect(onSubmit).toHaveBeenCalled();
  });

  it("uses the structured planet name instead of an encoded option id", () => {
    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "31b",
      prompt: "exhaust a planet to vote FOR",
      context: { subtype: "vote_exhaust_planet" },
      options: [
        {
          id: "exhaust|mecatol_rex",
          label: "",
          kind: "vote_planet",
          payload: { planet_name: "Mecatol Rex", votes: 6 },
        },
      ],
    };

    render(
      <AgendaBallotModal
        isOpen={true}
        choice={choice}
        viewerSeat="seat_1"
        onSubmit={vi.fn()}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByTestId("planet-card-exhaust|mecatol_rex")).toHaveTextContent("Mecatol Rex");
    expect(screen.queryByText("exhaust|mecatol_rex")).not.toBeInTheDocument();
  });

  it("renders vote_tiebreak stage with Speaker gavel and resolves tied outcome", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    const tiebreakChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "32",
      prompt: "speaker breaks the tie",
      context: {
        subtype: "vote_tiebreak",
      },
      options: [
        { id: "FOR", label: "FOR", kind: "tiebreak" },
        { id: "AGAINST", label: "AGAINST", kind: "tiebreak" },
      ],
    };

    render(
      <AgendaBallotModal
        isOpen={true}
        choice={tiebreakChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    expect(screen.getByTestId("agenda-ballot-title")).toHaveTextContent("Break the tie");
    expect(screen.getByText(/The council vote is tied!/i)).toBeInTheDocument();

    const resolveAgainstBtn = screen.getByTestId("tiebreak-opt-AGAINST");
    await act(async () => {
      fireEvent.click(resolveAgainstBtn);
    });
    expect(onSubmit).toHaveBeenCalledWith("AGAINST");
  });

  it("renders spectator notice when viewerSeat is not the active actor", () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    const choice: PendingChoiceDto = {
      actor: "seat_2",
      nonce: "33",
      prompt: "vote for which outcome",
      context: {
        subtype: "cast_vote",
      },
      options: [{ id: "FOR", label: "FOR" }],
    };

    render(
      <AgendaBallotModal
        isOpen={true}
        choice={choice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    expect(screen.getByTestId("spectator-agenda-notice")).toHaveTextContent(
      "Observing council voting in progress for Unknown participant...",
    );
  });

  it("displays error banner when lastError is provided", () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "34",
      prompt: "vote for which outcome",
      context: {
        subtype: "cast_vote",
      },
      options: [{ id: "FOR", label: "FOR" }],
    };

    render(
      <AgendaBallotModal
        isOpen={true}
        choice={choice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
        lastError="Council voting timeout"
      />,
    );

    expect(screen.getByTestId("agenda-error-banner")).toHaveTextContent("Council voting timeout");
  });
});
