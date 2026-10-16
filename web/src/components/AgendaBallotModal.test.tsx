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
    expect(screen.getByTestId("staged-votes-counter")).toHaveTextContent("+0 votes");

    const commitBtn = screen.getByTestId("commit-planet-votes-btn");
    expect(commitBtn).toBeDisabled();
    expect(screen.getByTestId("done-voting-planets-btn")).toHaveTextContent(
      "Finish without voting more",
    );
    expect(screen.queryByText("Done Voting")).not.toBeInTheDocument();

    // Stage Mecatol Rex
    const mecatolCard = screen.getByTestId("planet-card-Mecatol Rex");
    fireEvent.click(mecatolCard);
    expect(screen.getByTestId("staged-votes-counter")).toHaveTextContent("+6 votes");
    expect(mecatolCard).toHaveAttribute("aria-pressed", "true");
    expect(mecatolCard).toHaveTextContent(/^✓ /);
    expect(mecatolCard).toHaveTextContent("6 votes");
    expect(screen.getByTestId("planet-card-Jord")).toHaveAttribute("aria-pressed", "false");
    expect(commitBtn).not.toBeDisabled();

    // Stage Jord
    const jordCard = screen.getByTestId("planet-card-Jord");
    fireEvent.click(jordCard);
    expect(screen.getByTestId("staged-votes-counter")).toHaveTextContent("+8 votes");

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

  it("displays agenda card information when available", () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "35",
      prompt: "vote for which outcome",
      context: {
        subtype: "cast_vote",
        details: {
          agenda_card: {
            name: "Sling Relay",
            yes_outcome: "All players may move their ships in non-home systems.",
            no_outcome: "Players cannot move ships during this agenda phase.",
          },
        },
      },
      options: [
        { id: "FOR", label: "FOR", kind: "vote", payload: { current_votes: 3 } },
        { id: "AGAINST", label: "AGAINST", kind: "vote", payload: { current_votes: 2 } },
      ],
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

    expect(screen.getByTestId("agenda-card-display")).toBeInTheDocument();
    expect(screen.getByTestId("agenda-card-name")).toHaveTextContent("Sling Relay");
    expect(screen.getByTestId("agenda-yes-outcome")).toHaveTextContent(
      "All players may move their ships in non-home systems.",
    );
    expect(screen.getByTestId("agenda-no-outcome")).toHaveTextContent(
      "Players cannot move ships during this agenda phase.",
    );
  });

  it("does not display agenda card section when agenda_card is not provided", () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "36",
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

    expect(screen.queryByTestId("agenda-card-display")).not.toBeInTheDocument();
  });
});

describe("AgendaBallotModal map selection", () => {
  const exhaustChoice: PendingChoiceDto = {
    actor: "seat_1",
    nonce: "40",
    prompt: "exhaust planets to vote",
    context: { subtype: "vote_exhaust_planet" },
    options: [
      {
        id: "jord",
        label: "Exhaust Jord for 2 votes",
        kind: "vote_planet",
        payload: { planet: "jord" },
      },
      {
        id: "moll",
        label: "Exhaust Moll for 1 votes",
        kind: "vote_planet",
        payload: { planet: "moll" },
      },
      { id: "decline", label: "Done", kind: "decline" },
    ],
  };

  it("toggles a planet picked on the map into and out of the vote basket", () => {
    const onSelectOption = vi.fn();
    const props = {
      isOpen: true,
      choice: exhaustChoice,
      viewerSeat: "seat_1",
      onSubmit: vi.fn(),
      onClose: vi.fn(),
      onSelectOption,
    };
    const { rerender } = render(<AgendaBallotModal {...props} selectedOptionId="jord" />);
    expect(screen.getByTestId("planet-card-jord")).toHaveAttribute("data-staged", "true");
    expect(screen.getByTestId("staged-votes-counter")).toHaveTextContent("+2 votes");
    // The pick is consumed so a second click on the same planet registers again.
    expect(onSelectOption).toHaveBeenCalledWith("");

    rerender(<AgendaBallotModal {...props} selectedOptionId="" />);
    rerender(<AgendaBallotModal {...props} selectedOptionId="jord" />);
    expect(screen.getByTestId("planet-card-jord")).toHaveAttribute("data-staged", "false");
    expect(screen.getByTestId("staged-votes-counter")).toHaveTextContent("+0 votes");
  });

  it("ignores map picks that are not offered planets", () => {
    render(
      <AgendaBallotModal
        isOpen
        choice={exhaustChoice}
        viewerSeat="seat_1"
        onSubmit={vi.fn()}
        onClose={vi.fn()}
        selectedOptionId="decline"
      />,
    );
    expect(screen.getByTestId("staged-votes-counter")).toHaveTextContent("+0 votes");
  });
});
