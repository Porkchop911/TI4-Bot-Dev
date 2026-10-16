import { act } from "react";
import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import { ProductionBuilderDrawer } from "./ProductionBuilderDrawer.tsx";
import { PendingChoiceDto } from "../protocol/types.ts";
import { partialContext } from "../test/wire.ts";

describe("ProductionBuilderDrawer", () => {
  it("can stage a legally offered free-capacity batch", () => {
    const onQueueProduction = vi.fn();
    render(
      <ProductionBuilderDrawer
        choice={{
          actor: "seat_1",
          nonce: "free",
          prompt: "produce",
          context: partialContext({ subtype: "produce_unit", outstanding: [{ amount: 2, paid: 1 }] }),
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
      context: partialContext({
        subtype: "produce_unit",
        target: { System: "18" },
        outstanding: [{ amount: 5, paid: 2 }],
      }),
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
        { id: "decline", label: "Done Producing", kind: "decline", payload: {} },
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
      "0 / 8 (8 Left)",
    );

    const fighterBtn = screen.getByTestId("produce-unit-btn-produce|fighter");
    fireEvent.click(fighterBtn);
    expect(screen.getByTestId("produce-count-produce|fighter")).toHaveTextContent("1");
    expect(screen.getByTestId("production-capacity-counter")).toHaveTextContent(
      "4 / 5 Units (1 Left)",
    );
    expect(screen.getByTestId("production-resources-counter")).toHaveTextContent(
      "1 / 8 (7 Left)",
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
      "0 / 8 (8 Left)",
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
          context: partialContext({ subtype: "produce_unit", outstanding: [{ amount: 5, paid: 0 }] }),
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
    // The option shows the unit's name and its cost inline; the engine's "produce" prefix is dropped
    // from the control labels.
    expect(screen.getByRole("button", { name: "Add 2x fighter for 1" })).toBeInTheDocument();
    expect(screen.queryByRole("button", { name: "Add produce 2x fighter for 1" })).not.toBeInTheDocument();
    const fighterCost = within(screen.getByTestId("produce-option-build|fighter|2")).getByTestId("unit-stats");
    expect(fighterCost).toHaveTextContent("Cost 2 for 1");
    expect(within(fighterCost).getByRole("img", { name: "1 resource" })).toBeInTheDocument();
    fireEvent.click(fighter);
    fireEvent.click(fighter);
    expect(screen.getByTestId("production-capacity-counter")).toHaveTextContent(
      "4 / 5 Units (1 Left)",
    );
    expect(screen.getByTestId("production-resources-counter")).toHaveTextContent(
      "2 / 3 (1 Left)",
    );
    expect(carrier).toBeDisabled();
    expect(fighter).toBeDisabled();
    fireEvent.click(screen.getByRole("button", { name: "Remove 2x fighter for 1" }));
    expect(screen.getByTestId("production-resources-counter")).toHaveTextContent(
      "1 / 3 (2 Left)",
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
          context: partialContext({ subtype: "produce_unit", outstanding: [{ amount: 4, paid: 0 }] }),
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
      "0 / 4 (4 Left)",
    );
    fireEvent.click(plus);
    fireEvent.click(plus);
    expect(screen.getByTestId("production-resources-counter")).toHaveTextContent(
      "4 / 4 (0 Left)",
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
          context: partialContext({ subtype: "produce_unit", outstanding: [{ amount: 4, paid: 0 }] }),
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
      "1 / 2 (1 Left)",
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
          context: partialContext({ subtype: "produce_unit", outstanding: [{ amount: 3, paid: 0 }] }),
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
          context: partialContext({ subtype: "produce_unit", outstanding: [{ amount: 3, paid: 0 }] }),
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
      "0 / 0 (0 Left)",
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
      context: partialContext({
        subtype: "place_unit",
        target: { System: "18" },
      }),
      options: [
        { id: "place|space", label: "Place in space area", kind: "place", payload: {} },
        { id: "place|jord", label: "Place on Jord", kind: "place", payload: {} },
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
      context: partialContext({
        subtype: "produce_unit",
      }),
      options: [{ id: "decline", label: "Done", payload: {}, kind: "" }],
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
      context: partialContext({
        subtype: "produce_unit",
      }),
      options: [{ id: "decline", label: "Done", payload: {}, kind: "" }],
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

  it("displays fleet supply counter when the choice details carry it", () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "fleet-supply",
      prompt: "produce a unit",
      context: partialContext({
        subtype: "produce_unit",
        outstanding: [{ amount: 5, paid: 2 }],
      }),
      details: {
        fleet_supply: { used: 2, limit: 5 },
      },
      options: [
        {
          id: "build|fighter|1",
          kind: "produce",
          label: "Fighter",
          payload: { unit: "fighter", production_spent: 1, cost: 1, available_resources: 8 },
        },
      ],
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

    const fleetSupplyCounter = screen.getByTestId("fleet-supply-counter");
    expect(fleetSupplyCounter).toHaveTextContent("2 / 5");
  });

  it.each([
    [{ used: 3, limit: 4 }, "3 / 4"],
    [{ used: 6, limit: 10000, unlimited: true }, "6 / unlimited"],
  ])("reads fleet supply %j from the server's choice details", (fleetSupply, expected) => {
    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "fleet-supply-details",
      prompt: "produce a unit",
      context: partialContext({ subtype: "produce_unit", outstanding: [{ amount: 5, paid: 2 }] }),
      details: { fleet_supply: fleetSupply },
      options: [
        {
          id: "build|fighter|1",
          kind: "produce",
          label: "Fighter",
          payload: { unit: "fighter", production_spent: 1, cost: 1, available_resources: 8 },
        },
      ],
    };
    render(
      <ProductionBuilderDrawer
        isOpen={true}
        choice={choice}
        viewerSeat="seat_1"
        onSubmit={vi.fn()}
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByTestId("fleet-supply-counter")).toHaveTextContent(expected);
  });

  it("displays 'Data unavailable' for fleet supply when not in context", () => {
    const onSubmit = vi.fn();
    const onClose = vi.fn();

    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "no-fleet-supply",
      prompt: "produce a unit",
      context: partialContext({
        subtype: "produce_unit",
        outstanding: [{ amount: 5, paid: 2 }],
      }),
      options: [
        {
          id: "build|fighter|1",
          kind: "produce",
          label: "Fighter",
          payload: { unit: "fighter", production_spent: 1, cost: 1, available_resources: 8 },
        },
      ],
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

    const fleetSupplyCounter = screen.getByTestId("fleet-supply-counter");
    expect(fleetSupplyCounter).toHaveTextContent("Data unavailable");
  });

  describe("Amalgamation exchange", () => {
    const cabal: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "cabal",
      prompt: "produce in 14",
      context: partialContext({
        subtype: "produce_unit",
        target: { System: "14" },
        outstanding: [{ amount: 3, paid: 0 }],
      }),
      options: [
        {
          id: "build|carrier|1",
          kind: "produce",
          label: "produce 1x carrier for 3",
          payload: { unit: "carrier", count: 1, cost: 3, printed_cost: 3, discount: 0, production_spent: 1, available_resources: 3 },
        },
        {
          id: "exchange|carrier",
          kind: "produce",
          label: "produce 1x carrier by returning a captured carrier",
          payload: { unit: "carrier", count: 1, cost: 0, printed_cost: 3, discount: 0, production_spent: 1, available_resources: 3, exchange: true },
        },
        { id: "done_producing", kind: "decline", label: "done", payload: {} },
      ],
    };
    const open = (props: Partial<React.ComponentProps<typeof ProductionBuilderDrawer>>) =>
      render(
        <ProductionBuilderDrawer
          choice={cabal}
          viewerSeat="seat_1"
          onSubmit={vi.fn()}
          onClose={vi.fn()}
          isOpen
          {...props}
        />,
      );

    it("offers the exchange as its own row, costs it nothing and sends exchange true", async () => {
      const onSubmitBatch = vi.fn().mockResolvedValue(undefined);
      open({ onSubmitBatch });
      expect(screen.getByTestId("produce-exchange-tag-exchange|carrier")).toBeInTheDocument();
      expect(screen.queryByTestId("produce-exchange-tag-build|carrier|1")).toBeNull();
      fireEvent.click(screen.getByTestId("produce-unit-btn-exchange|carrier"));
      expect(screen.getByTestId("production-resources-counter")).toHaveTextContent("0 / 3");
      expect(screen.getByTestId("production-capacity-counter")).toHaveTextContent("1 / 3");
      await act(async () => {
        fireEvent.click(screen.getByRole("button", { name: "Confirm builds" }));
      });
      expect(onSubmitBatch).toHaveBeenCalledWith({
        kind: "production",
        destination: "14",
        steps: [{ kind: "produce", unit: "carrier", count: 1, exchange: true }],
      });
    });

    it("keeps the paid build of the same type a plain step", async () => {
      const onSubmitBatch = vi.fn().mockResolvedValue(undefined);
      open({ onSubmitBatch });
      fireEvent.click(screen.getByTestId("produce-unit-btn-build|carrier|1"));
      expect(screen.getByTestId("production-resources-counter")).toHaveTextContent("3 / 3");
      await act(async () => {
        fireEvent.click(screen.getByRole("button", { name: "Confirm builds" }));
      });
      expect(onSubmitBatch).toHaveBeenCalledWith({
        kind: "production",
        destination: "14",
        steps: [{ kind: "produce", unit: "carrier", count: 1 }],
      });
    });

    it("queues an exchange under its own key so it is never mistaken for the paid build", () => {
      const onQueueProduction = vi.fn();
      open({ onQueueProduction });
      fireEvent.click(screen.getByTestId("produce-unit-btn-exchange|carrier"));
      fireEvent.click(screen.getByTestId("produce-unit-btn-build|carrier|1"));
      fireEvent.click(screen.getByRole("button", { name: "Confirm builds" }));
      expect(onQueueProduction).toHaveBeenCalledWith(["carrier", "exchange|carrier"]);
    });
  });

  it("splits the resource budget into planets and trade goods", () => {
    render(
      <ProductionBuilderDrawer
        choice={{
          actor: "seat_1",
          nonce: "split",
          prompt: "produce",
          context: partialContext({ subtype: "produce_unit", outstanding: [{ amount: 4, paid: 0 }] }),
          options: [
            {
              id: "build|fighter|2",
              kind: "produce",
              label: "Fighter",
              payload: { unit: "fighter", cost: 1, available_resources: 9 },
            },
          ],
        }}
        viewerSeat="seat_1"
        tradeGoods={7}
        onSubmit={vi.fn()}
        onClose={vi.fn()}
        isOpen
      />,
    );
    expect(screen.getByTestId("production-split-planets")).toHaveTextContent("2");
    expect(screen.getByTestId("production-split-tg")).toHaveTextContent("7");
    expect(screen.getByTestId("trade-good-sources-empty")).toBeInTheDocument();
  });
});
