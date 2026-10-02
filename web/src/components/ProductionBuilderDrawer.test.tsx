import { act } from "react";
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { ProductionBuilderDrawer } from "./ProductionBuilderDrawer.tsx";
import { PendingChoiceDto } from "../protocol/types.ts";

describe("ProductionBuilderDrawer", () => {
  it("can stage a legally offered free-capacity batch", () => {
    const onQueueProduction = vi.fn();
    render(
      <ProductionBuilderDrawer
        choice={{
          actor: "seat_1",
          nonce: "free",
          prompt: "produce",
          context: { subtype: "produce_unit", outstanding: [{ amount: 2, paid: 1 }] },
          options: [
            {
              id: "build|fighter|1",
              kind: "produce",
              label: "Fighter",
              payload: { unit: "fighter", production_spent: 0, cost: 1, available_resources: 1 },
            },
          ],
        }}
        viewerSeat="seat_1"
        onSubmit={vi.fn()}
        onClose={vi.fn()}
        isOpen
        onQueueProduction={onQueueProduction}
      />,
    );
    fireEvent.click(screen.getByTestId("produce-unit-btn-build|fighter|1"));
    expect(screen.getByRole("button", { name: "Confirm builds" })).toBeEnabled();
    fireEvent.click(screen.getByRole("button", { name: "Confirm builds" }));
    expect(onQueueProduction).toHaveBeenCalledWith(["fighter"]);
  });
  it("stages production with +/- and reset before submitting and shows available resources", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    const produceChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "50",
      prompt: "produce a unit",
      context: {
        subtype: "produce_unit",
        target: { System: "18" },
        outstanding: [{ amount: 5, paid: 2 }],
      },
      options: [
        {
          id: "produce|fighter",
          label: "Fighter (0.5 cost)",
          kind: "produce",
          payload: { unit: "fighter", production_spent: 2, cost: 1, available_resources: 8 },
        },
        {
          id: "produce|carrier",
          label: "Carrier (3 cost)",
          kind: "produce",
          payload: { unit: "carrier", production_spent: 1, cost: 3, available_resources: 8 },
        },
        { id: "decline", label: "Done Producing", kind: "decline" },
      ],
    };

    const onQueueProduction = vi.fn();
    render(
      <ProductionBuilderDrawer
        isOpen={true}
        choice={produceChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
        onQueueProduction={onQueueProduction}
      />,
    );

    expect(screen.getByTestId("production-drawer-title")).toHaveTextContent("Produce units");
    expect(screen.getByTestId("production-capacity-counter")).toHaveTextContent(
      "2 / 5 Units (3 Left)",
    );
    expect(screen.getByTestId("production-resources-counter")).toHaveTextContent(
      "0 / 8 Resources (8 Left)",
    );

    const fighterBtn = screen.getByTestId("produce-unit-btn-produce|fighter");
    fireEvent.click(fighterBtn);
    expect(screen.getByTestId("produce-count-produce|fighter")).toHaveTextContent("1");
    expect(screen.getByTestId("production-capacity-counter")).toHaveTextContent(
      "4 / 5 Units (1 Left)",
    );
    expect(screen.getByTestId("production-resources-counter")).toHaveTextContent(
      "1 / 8 Resources (7 Left)",
    );
    expect(fighterBtn).toBeDisabled(); // Only one capacity left.
    fireEvent.click(screen.getByRole("button", { name: "Remove Fighter (0.5 cost)" }));
    expect(screen.getByTestId("produce-count-produce|fighter")).toHaveTextContent("0");
    expect(screen.getByTestId("production-capacity-counter")).toHaveTextContent(
      "2 / 5 Units (3 Left)",
    );
    fireEvent.click(fighterBtn);
    fireEvent.click(screen.getByRole("button", { name: "Reset selection" }));
    expect(onSubmit).not.toHaveBeenCalled();
    expect(screen.getByTestId("produce-count-produce|fighter")).toHaveTextContent("0");
    expect(screen.getByTestId("production-resources-counter")).toHaveTextContent(
      "0 / 8 Resources (8 Left)",
    );

    fireEvent.click(fighterBtn);
    fireEvent.click(screen.getByTestId("produce-unit-btn-produce|carrier"));
    expect(screen.getByTestId("produce-unit-btn-produce|carrier")).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Confirm builds" }));
    expect(onQueueProduction).toHaveBeenCalledWith(["fighter", "carrier"]);
    expect(onSubmit).not.toHaveBeenCalled();

    const doneBtn = screen.getByTestId("done-producing-btn");
    await act(async () => {
      fireEvent.click(doneBtn);
    });
    expect(onSubmit).toHaveBeenCalledWith("decline");
  });

  it("refuses batches beyond available resources, restores the budget on minus, and removes the redundant produce prefix", () => {
    const onQueueProduction = vi.fn();
    render(
      <ProductionBuilderDrawer
        choice={{
          actor: "seat_1",
          nonce: "budget",
          prompt: "produce",
          context: { subtype: "produce_unit", outstanding: [{ amount: 5, paid: 0 }] },
          options: [
            {
              id: "build|fighter|2",
              kind: "produce",
              label: "produce 2x fighter for 1",
              payload: { unit: "fighter", production_spent: 2, cost: 1, available_resources: 3 },
            },
            {
              id: "build|carrier|1",
              kind: "produce",
              label: "produce 1x carrier for 3",
              payload: { unit: "carrier", production_spent: 1, cost: 3, available_resources: 3 },
            },
          ],
        }}
        viewerSeat="seat_1"
        onSubmit={vi.fn()}
        onClose={vi.fn()}
        isOpen
        onQueueProduction={onQueueProduction}
      />,
    );
    const fighter = screen.getByTestId("produce-unit-btn-build|fighter|2");
    const carrier = screen.getByTestId("produce-unit-btn-build|carrier|1");
    expect(screen.getByText("2x fighter for 1")).toBeInTheDocument();
    expect(screen.queryByText("produce 2x fighter for 1")).not.toBeInTheDocument();
    fireEvent.click(fighter);
    fireEvent.click(fighter);
    expect(screen.getByTestId("production-capacity-counter")).toHaveTextContent(
      "4 / 5 Units (1 Left)",
    );
    expect(screen.getByTestId("production-resources-counter")).toHaveTextContent(
      "2 / 3 Resources (1 Left)",
    );
    expect(carrier).toBeDisabled();
    expect(fighter).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Remove 2x fighter for 1" }));
    expect(screen.getByTestId("production-resources-counter")).toHaveTextContent(
      "1 / 3 Resources (2 Left)",
    );
    expect(fighter).toBeEnabled();
    expect(carrier).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Confirm builds" }));
    expect(onQueueProduction).toHaveBeenCalledWith(["fighter"]);
  });

  it("counts carried production credit once across the resource budget", () => {
    render(
      <ProductionBuilderDrawer
        choice={{
          actor: "seat_1",
          nonce: "credit",
          prompt: "produce",
          context: { subtype: "produce_unit", outstanding: [{ amount: 4, paid: 0 }] },
          options: [
            {
              id: "build|fighter|1",
              kind: "produce",
              label: "produce 1x fighter for 2",
              payload: {
                unit: "fighter",
                production_spent: 1,
                cost: 2,
                credit: 1,
                available_resources: 3,
              },
            },
          ],
        }}
        viewerSeat="seat_1"
        onSubmit={vi.fn()}
        onClose={vi.fn()}
        isOpen
        onQueueProduction={vi.fn()}
      />,
    );
    const plus = screen.getByTestId("produce-unit-btn-build|fighter|1");
    expect(screen.getByTestId("production-resources-counter")).toHaveTextContent(
      "0 / 4 Resources (4 Left)",
    );
    fireEvent.click(plus);
    fireEvent.click(plus);
    expect(screen.getByTestId("production-resources-counter")).toHaveTextContent(
      "4 / 4 Resources (0 Left)",
    );
    expect(plus).toBeDisabled();
  });

  it("does not spend a one-time discount twice when planning repeated builds", () => {
    render(
      <ProductionBuilderDrawer
        choice={{
          actor: "seat_1",
          nonce: "discount",
          prompt: "produce",
          context: { subtype: "produce_unit", outstanding: [{ amount: 4, paid: 0 }] },
          options: [
            {
              id: "build|fighter|1",
              kind: "produce",
              label: "produce 1x fighter for 1",
              payload: {
                unit: "fighter",
                production_spent: 1,
                cost: 1,
                printed_cost: 2,
                discount: 1,
                available_resources: 2,
              },
            },
          ],
        }}
        viewerSeat="seat_1"
        onSubmit={vi.fn()}
        onClose={vi.fn()}
        isOpen
        onQueueProduction={vi.fn()}
      />,
    );
    const plus = screen.getByTestId("produce-unit-btn-build|fighter|1");
    fireEvent.click(plus);
    expect(screen.getByTestId("production-resources-counter")).toHaveTextContent(
      "1 / 2 Resources (1 Left)",
    );
    expect(plus).toBeDisabled(); // A second batch would cost the full 2, not the discounted 1.
  });

  it("keeps builds disabled if the exact resource budget is missing", () => {
    render(
      <ProductionBuilderDrawer
        choice={{
          actor: "seat_1",
          nonce: "unknown",
          prompt: "produce",
          context: { subtype: "produce_unit", outstanding: [{ amount: 3, paid: 0 }] },
          options: [
            {
              id: "build|carrier|1",
              kind: "produce",
              label: "produce 1x carrier for 3",
              payload: { unit: "carrier", production_spent: 1, cost: 3 },
            },
          ],
        }}
        viewerSeat="seat_1"
        onSubmit={vi.fn()}
        onClose={vi.fn()}
        isOpen
        onQueueProduction={vi.fn()}
      />,
    );
    expect(screen.getByTestId("production-resources-counter")).toHaveTextContent("Unavailable");
    expect(screen.getByTestId("produce-unit-btn-build|carrier|1")).toBeDisabled();
  });

  it("keeps a free production use available at zero resources", () => {
    render(
      <ProductionBuilderDrawer
        choice={{
          actor: "seat_1",
          nonce: "free-use",
          prompt: "produce",
          context: { subtype: "produce_unit", outstanding: [{ amount: 3, paid: 0 }] },
          options: [
            {
              id: "build|fighter|1",
              kind: "produce",
              label: "produce 1x fighter for 0",
              payload: {
                unit: "fighter",
                production_spent: 1,
                cost: 0,
                printed_cost: 2,
                discount: 2,
                free_this_use: true,
                available_resources: 0,
              },
            },
          ],
        }}
        viewerSeat="seat_1"
        onSubmit={vi.fn()}
        onClose={vi.fn()}
        isOpen
        onQueueProduction={vi.fn()}
      />,
    );
    const plus = screen.getByTestId("produce-unit-btn-build|fighter|1");
    fireEvent.click(plus);
    fireEvent.click(plus);
    expect(screen.getByTestId("production-resources-counter")).toHaveTextContent(
      "0 / 0 Resources (0 Left)",
    );
    expect(plus).toBeEnabled();
  });

  it("renders place_unit mode and handles spot selection", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onClose = vi.fn();

    const placeChoice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "51",
      prompt: "place the fighter",
      context: {
        subtype: "place_unit",
        target: { System: "18" },
      },
      options: [
        { id: "place|space", label: "Place in space area", kind: "place" },
        { id: "place|jord", label: "Place on Jord", kind: "place" },
      ],
    };

    render(
      <ProductionBuilderDrawer
        isOpen={true}
        choice={placeChoice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    expect(screen.getByTestId("production-drawer-title")).toHaveTextContent("Choose a placement");

    const spaceBtn = screen.getByTestId("place-spot-btn-place|space");
    await act(async () => {
      fireEvent.click(spaceBtn);
    });
    expect(onSubmit).toHaveBeenCalledWith("place|space");
  });

  it("renders spectator notice when viewerSeat is not active actor", () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    const choice: PendingChoiceDto = {
      actor: "seat_2",
      nonce: "52",
      prompt: "produce a unit",
      context: {
        subtype: "produce_unit",
      },
      options: [{ id: "decline", label: "Done" }],
    };

    render(
      <ProductionBuilderDrawer
        isOpen={true}
        choice={choice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
      />,
    );

    expect(screen.getByTestId("spectator-production-notice")).toHaveTextContent(
      "Observing unit production in progress for Unknown participant...",
    );
  });

  it("displays error banner when lastError is set", () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "53",
      prompt: "produce a unit",
      context: {
        subtype: "produce_unit",
      },
      options: [{ id: "decline", label: "Done" }],
    };

    render(
      <ProductionBuilderDrawer
        isOpen={true}
        choice={choice}
        viewerSeat="seat_1"
        onSubmit={onSubmit}
        onClose={onClose}
        lastError="Insufficient production capacity"
      />,
    );

    expect(screen.getByTestId("production-error-banner")).toHaveTextContent(
      "Insufficient production capacity",
    );
  });
});
