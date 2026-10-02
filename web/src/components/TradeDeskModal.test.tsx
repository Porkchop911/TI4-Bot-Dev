import { act } from "react";
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { TradeDeskModal } from "./TradeDeskModal.tsx";
import { PendingChoiceDto } from "../protocol/types.ts";

describe("TradeDeskModal", () => {
  it("renders propose mode with categorized tabs and handles deal proposal", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    const proposeChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "20",
      prompt: "Propose a transaction",
      context: {
        subtype: "propose_transaction",
        target: { Player: "seat_2" },
      },
      options: [
        {
          id: "cc2",
          label: "swap 2 commodities each",
          kind: "offer",
          payload: { net: 2, their_net: 2 },
        },
        {
          id: "ct2:2",
          label: "give 2 commodities for 2 trade goods",
          kind: "offer",
          payload: { net: 0, their_net: 0 },
        },
        {
          id: "pnmilitary_support:sol:3",
          label: "Military Support for 3 TG",
          kind: "offer",
          payload: { net: 3, their_net: 1 },
        },
        { id: "decline", label: "Offer nothing", kind: "decline" },
      ],
    };

    render(
      <TradeDeskModal
        isOpen={true}
        choice={proposeChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    expect(screen.getByTestId("trade-desk-title")).toHaveTextContent("Propose a trade");
    expect(screen.getByTestId("trade-tab-commodity_swap")).toBeInTheDocument();
    expect(screen.getByTestId("trade-tab-goods_exchange")).toBeInTheDocument();
    expect(screen.getByTestId("trade-tab-promissory")).toBeInTheDocument();

    // Default tab is commodity_swap
    expect(screen.getByTestId("trade-opt-cc2")).toBeInTheDocument();
    expect(screen.queryByTestId("trade-opt-ct2:2")).not.toBeInTheDocument();

    // Switch to goods_exchange tab
    fireEvent.click(screen.getByTestId("trade-tab-goods_exchange"));
    expect(screen.getByTestId("trade-opt-ct2:2")).toBeInTheDocument();
    expect(screen.queryByTestId("trade-opt-cc2")).not.toBeInTheDocument();

    // Switch to promissory tab
    fireEvent.click(screen.getByTestId("trade-tab-promissory"));
    const pnOpt = screen.getByTestId("trade-opt-pnmilitary_support:sol:3");
    expect(pnOpt).toBeInTheDocument();

    // Propose button is disabled before selecting an offer
    const proposeBtn = screen.getByTestId("propose-trade-btn");
    expect(proposeBtn).toBeDisabled();

    // Select the promissory note offer
    fireEvent.click(pnOpt);
    expect(screen.getByTestId("selected-trade-summary")).toHaveTextContent(
      "Military Support for 3 TG",
    );
    expect(proposeBtn).not.toBeDisabled();

    // Click propose deal
    await act(async () => {
      fireEvent.click(proposeBtn);
    });
    expect(onSubmit).toHaveBeenCalledWith("pnmilitary_support:sol:3");
  });

  it("handles offer nothing / decline in propose mode", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    const proposeChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "21",
      prompt: "Propose a transaction",
      context: {
        subtype: "propose_transaction",
        target: { Player: "seat_2" },
      },
      options: [
        { id: "cc1", label: "swap 1 commodity each", kind: "offer" },
        { id: "decline", label: "Offer nothing", kind: "decline" },
      ],
    };

    render(
      <TradeDeskModal
        isOpen={true}
        choice={proposeChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    const declineBtn = screen.getByTestId("decline-trade-btn");
    await act(async () => {
      fireEvent.click(declineBtn);
    });
    expect(onSubmit).toHaveBeenCalledWith("decline");
  });

  it("renders answer mode and handles accept, counter-offer, and refuse", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    const answerChoice: PendingChoiceDto = {
      actor: "seat_2",
      nonce: "22",
      prompt: "seat_1 offers 2 commodities for 2 trade goods -- accept?",
      context: {
        subtype: "answer_transaction",
        target: { Player: "seat_1" },
      },
      options: [
        { id: "accept", label: "Accept", kind: "answer", payload: { net: 0 } },
        { id: "refuse", label: "Refuse", kind: "decline" },
        { id: "counter", label: "Counter-Offer", kind: "answer" },
      ],
    };

    render(
      <TradeDeskModal
        isOpen={true}
        choice={answerChoice}
        viewerSeat="seat_2"
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    expect(screen.getByTestId("trade-desk-title")).toHaveTextContent("Answer the trade offer");
    expect(screen.getByTestId("trade-desk-modal")).toHaveTextContent("seat_1 offers 2 commodities");

    const acceptBtn = screen.getByTestId("answer-opt-accept");
    const counterBtn = screen.getByTestId("answer-opt-counter");
    const refuseBtn = screen.getByTestId("answer-opt-refuse");

    expect(acceptBtn).toBeInTheDocument();
    expect(counterBtn).toBeInTheDocument();
    expect(refuseBtn).toBeInTheDocument();

    await act(async () => {
      fireEvent.click(acceptBtn);
    });
    expect(onSubmit).toHaveBeenCalledWith("accept");
  });

  it("renders spectator notice when viewerSeat is not the active actor", () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    const choice: PendingChoiceDto = {
      actor: "seat_2",
      nonce: "23",
      prompt: "Propose transaction",
      context: {
        subtype: "propose_transaction",
        target: { Player: "seat_3" },
      },
      options: [{ id: "decline", label: "Decline" }],
    };

    render(
      <TradeDeskModal
        isOpen={true}
        choice={choice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    expect(screen.getByTestId("spectator-trade-notice")).toHaveTextContent(
      "Observing bilateral trade negotiations between Unknown participant and Unknown participant...",
    );
  });

  it("displays error alert when lastError is provided", () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "24",
      prompt: "Propose transaction",
      context: {
        subtype: "propose_transaction",
        target: { Player: "seat_2" },
      },
      options: [{ id: "decline", label: "Decline" }],
    };

    render(
      <TradeDeskModal
        isOpen={true}
        choice={choice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
        lastError="Cannot afford transaction"
      />,
    );

    expect(screen.getByTestId("trade-error-banner")).toHaveTextContent("Cannot afford transaction");
  });
});
