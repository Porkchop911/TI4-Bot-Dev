import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { PaymentDrawer } from "./PaymentDrawer.tsx";
import { PaymentDraftProvider } from "../presentation/PaymentDraftContext.tsx";
import { PendingChoiceDto, PlayerView } from "../protocol/types.ts";

const mockPaymentChoice: PendingChoiceDto = {
  prompt: "pay 4 more resources",
  actor: "p1",
  nonce: "nonce_pay_1",
  context: {
    subtype: "pay_resources",
    outstanding: [{ kind: "resources", amount: 4, paid: 0 }],
  },
  options: [
    {
      id: "exhaust|jord",
      label: "exhaust jord for 4 resources",
      kind: "pay",
      payload: { worth: 4, owed: 4, kind: "resources", source: "planet", planet_name: "Jord" },
    },
    {
      id: "exhaust|arinam",
      label: "exhaust arinam for 1 resources",
      kind: "pay",
      payload: { worth: 1, owed: 4, kind: "resources", source: "planet", planet_name: "Arinam" },
    },
    {
      id: "trade_good",
      label: "spend a trade good",
      kind: "pay",
      payload: { worth: 1, owed: 4, kind: "resources" },
    },
    {
      id: "decline",
      label: "Cancel Payment",
      kind: "decline",
    },
  ],
};

const mockPlayer: PlayerView = {
  id: "p1",
  faction: "sol",
  victory_points: 0,
  trade_goods: 3,
  commodities: 2,
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
  action_cards_count: 2,
  secret_objectives_count: 1,
  leaders: {},
};

describe("PaymentDrawer Component", () => {
  it("does not render when choice is null", () => {
    const { container } = render(
      <PaymentDrawer
        choice={null}
        player={mockPlayer}
        onSubmit={vi.fn()}
        isOpen={true}
        onClose={vi.fn()}
      />,
    );
    expect(container.firstChild).toBeNull();
  });

  it("renders ready planets and trade good stepper with initial zero committed", () => {
    render(
      <PaymentDrawer
        choice={mockPaymentChoice}
        player={mockPlayer}
        onSubmit={vi.fn()}
        isOpen={true}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByTestId("payment-drawer-title").querySelector("[aria-label]")).toHaveAttribute("aria-label", "4 resources");
    expect(screen.getByTestId("payment-drawer-title")).toHaveTextContent("Pay");
    expect(screen.getByTestId("committed-amount").querySelector("[aria-label]")).toHaveAttribute("aria-label", "0 resources");
    expect(screen.getByTestId("planet-card-exhaust|jord")).toBeInTheDocument();
    expect(screen.getByTestId("planet-card-exhaust|arinam")).toBeInTheDocument();
    expect(screen.getByTestId("trade-goods-stepper")).toBeInTheDocument();

    const confirmBtn = screen.getByTestId("confirm-payment-btn");
    expect(confirmBtn).toBeDisabled();
  });

  it("represents planet selection with a class and data state instead of an inline style", () => {
    render(
      <PaymentDrawer
        choice={mockPaymentChoice}
        player={mockPlayer}
        viewerSeat="p1"
        onSubmit={vi.fn()}
        isOpen
        onClose={vi.fn()}
      />,
    );

    const planet = screen.getByTestId("planet-card-exhaust|jord");
    expect(planet).toHaveClass("payment-drawer__planet");
    expect(planet).toHaveAttribute("data-selected", "false");
    fireEvent.click(planet.querySelector("input")!);
    expect(planet).toHaveAttribute("data-selected", "true");
    expect(planet).toHaveClass("card--selected");
    expect(planet).not.toHaveAttribute("style");
  });

  it("uses the structured planet name rather than parsing its option id", () => {
    const choice: PendingChoiceDto = {
      ...mockPaymentChoice,
      options: [
        {
          id: "exhaust|mecatol_rex",
          label: "",
          kind: "pay",
          payload: { worth: 6, planet_name: "Mecatol Rex" },
        },
      ],
    };

    render(
      <PaymentDrawer
        choice={choice}
        player={mockPlayer}
        onSubmit={vi.fn()}
        isOpen={true}
        onClose={vi.fn()}
      />,
    );

    expect(screen.getByTestId("planet-card-exhaust|mecatol_rex")).toHaveTextContent("Mecatol Rex");
    expect(screen.queryByText("mecatol_rex")).not.toBeInTheDocument();
  });

  it("keeps confirm disabled until the staged amount covers the bill", () => {
    render(
      <PaymentDrawer
        choice={mockPaymentChoice}
        player={mockPlayer}
        onSubmit={vi.fn()}
        isOpen={true}
        onClose={vi.fn()}
      />,
    );

    const arinam = screen.getByTestId("planet-card-exhaust|arinam");
    fireEvent.click(arinam.querySelector('input[type="checkbox"]')!);

    const confirmBtn = screen.getByTestId("confirm-payment-btn");
    expect(screen.getByTestId("committed-amount").querySelector("[aria-label]")).toHaveAttribute("aria-label", "1 resource");
    expect(confirmBtn).toBeDisabled();
    expect(confirmBtn).toHaveTextContent("Stage 3 more to pay");
  });

  it("toggles planet selection and enables confirm when debt is met", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(
      <PaymentDrawer
        choice={mockPaymentChoice}
        player={mockPlayer}
        onSubmit={onSubmit}
        isOpen={true}
        onClose={vi.fn()}
      />,
    );

    const jordCard = screen.getByTestId("planet-card-exhaust|jord");
    const jordCheckbox = jordCard.querySelector('input[type="checkbox"]')!;

    // Select Jord (+4)
    fireEvent.click(jordCheckbox);

    expect(screen.getByTestId("committed-amount").querySelector("[aria-label]")).toHaveAttribute("aria-label", "4 resources");
    const confirmBtn = screen.getByTestId("confirm-payment-btn");
    expect(confirmBtn).not.toBeDisabled();

    // Confirm single payment
    await act(async () => {
      fireEvent.click(confirmBtn);
    });

    expect(onSubmit).toHaveBeenCalledWith("exhaust|jord");
  });

  it("supports spending trade goods and shows retained credit on overpayment", () => {
    render(
      <PaymentDrawer
        choice={mockPaymentChoice}
        player={mockPlayer}
        onSubmit={vi.fn()}
        isOpen={true}
        onClose={vi.fn()}
      />,
    );

    // Select Arinam (+1)
    const arinamCard = screen.getByTestId("planet-card-exhaust|arinam");
    fireEvent.click(arinamCard.querySelector('input[type="checkbox"]')!);
    expect(screen.getByTestId("committed-amount").querySelector("[aria-label]")).toHaveAttribute("aria-label", "1 resource");

    // Increment Trade Goods twice (+2)
    const tgIncBtn = screen.getByTestId("tg-increment-btn");
    fireEvent.click(tgIncBtn);
    fireEvent.click(tgIncBtn);
    expect(screen.getByTestId("tg-count")).toHaveTextContent("2");
    expect(screen.getByTestId("committed-amount").querySelector("[aria-label]")).toHaveAttribute("aria-label", "3 resources");

    // Also select Jord (+4), bringing total to 7 (overpayment: 3 credit)
    const jordCard = screen.getByTestId("planet-card-exhaust|jord");
    fireEvent.click(jordCard.querySelector('input[type="checkbox"]')!);
    expect(screen.getByTestId("committed-amount").querySelector("[aria-label]")).toHaveAttribute("aria-label", "7 resources");
    expect(screen.getByText(/Potential overpayment/)).toBeInTheDocument();
    expect(screen.getByLabelText("plus 3 resources")).toBeInTheDocument();
  });

  it("calls onSubmit with decline option when cancel is clicked", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(
      <PaymentDrawer
        choice={mockPaymentChoice}
        player={mockPlayer}
        onSubmit={onSubmit}
        isOpen={true}
        onClose={vi.fn()}
      />,
    );

    const declineBtn = screen.getByTestId("decline-payment-btn");
    expect(declineBtn).toHaveTextContent("Cancel Payment");
    await act(async () => {
      fireEvent.click(declineBtn);
    });
    expect(onSubmit).toHaveBeenCalledWith("decline");
  });

  it("stages only one variant of a planet that is offered as several options", () => {
    const variants: PendingChoiceDto = {
      ...mockPaymentChoice,
      prompt: "pay 3 more influence",
      options: [
        {
          id: "exhaust|archonren",
          label: "exhaust archonren for 2 resources",
          kind: "pay",
          payload: { worth: 2, owed: 3, kind: "resources", source: "planet", planet_name: "Archon Ren" },
        },
        {
          id: "exhaust|archonren|influence",
          label: "exhaust archonren for 3 influence",
          kind: "pay",
          payload: { worth: 3, owed: 3, kind: "influence", source: "planet", planet_name: "Archon Ren" },
        },
        { id: "decline", label: "Cancel Payment", kind: "decline" },
      ],
    };
    render(
      <PaymentDrawer
        choice={variants}
        player={mockPlayer}
        viewerSeat="p1"
        onSubmit={vi.fn()}
        isOpen
        onClose={vi.fn()}
      />,
    );
    const resources = screen.getByTestId("planet-card-exhaust|archonren");
    const influence = screen.getByTestId("planet-card-exhaust|archonren|influence");
    fireEvent.click(resources.querySelector("input")!);
    expect(resources).toHaveAttribute("data-selected", "true");
    fireEvent.click(influence.querySelector("input")!);
    expect(influence).toHaveAttribute("data-selected", "true");
    expect(resources).toHaveAttribute("data-selected", "false");
  });

  it("Auto-pay stages the cheapest covering planets and nothing is submitted until confirm", () => {
    const onSubmit = vi.fn();
    render(
      <PaymentDrawer
        choice={mockPaymentChoice}
        player={mockPlayer}
        viewerSeat="p1"
        onSubmit={onSubmit}
        isOpen
        onClose={vi.fn()}
      />,
    );
    expect(screen.getByTestId("payment-problem")).toHaveTextContent(/Short by 4/);
    fireEvent.click(screen.getByTestId("auto-pay-btn"));
    expect(screen.getByTestId("committed-amount").querySelector("[aria-label]")).toHaveAttribute("aria-label", "4 resources");
    expect(screen.getByTestId("planet-card-exhaust|jord")).toHaveAttribute("data-selected", "true");
    expect(screen.getByTestId("confirm-payment-btn")).toBeEnabled();
    expect(screen.queryByTestId("payment-problem")).not.toBeInTheDocument();
    expect(onSubmit).not.toHaveBeenCalled();
  });

  it("shows a staged selection made on the map through the shared draft", () => {
    const togglePlanet = vi.fn();
    render(
      <PaymentDraftProvider
        value={{
          draft: { planetIds: ["exhaust|arinam"], tradeGoods: 0 },
          togglePlanet,
          setTradeGoods: vi.fn(),
          setDraft: vi.fn(),
          reset: vi.fn(),
        }}
      >
        <PaymentDrawer
          choice={mockPaymentChoice}
          player={mockPlayer}
          viewerSeat="p1"
          onSubmit={vi.fn()}
          isOpen
          onClose={vi.fn()}
        />
      </PaymentDraftProvider>,
    );
    expect(screen.getByTestId("planet-card-exhaust|arinam")).toHaveAttribute("data-selected", "true");
    expect(screen.getByTestId("committed-amount").querySelector("[aria-label]")).toHaveAttribute("aria-label", "1 resource");
    fireEvent.click(screen.getByTestId("planet-card-exhaust|jord").querySelector("input")!);
    expect(togglePlanet).toHaveBeenCalledWith("exhaust|jord");
  });

  it("Pick on map closes the list", () => {
    const onClose = vi.fn();
    render(
      <PaymentDrawer
        choice={mockPaymentChoice}
        player={mockPlayer}
        viewerSeat="p1"
        onSubmit={vi.fn()}
        isOpen
        onClose={onClose}
      />,
    );
    fireEvent.click(screen.getByTestId("pick-on-map-btn"));
    expect(onClose).toHaveBeenCalled();
  });
});
