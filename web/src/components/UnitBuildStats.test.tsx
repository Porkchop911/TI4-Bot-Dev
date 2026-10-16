import { describe, it, expect } from "vitest";
import { render, screen, within, fireEvent } from "@testing-library/react";
import { UnitBuildStats, costText } from "./UnitBuildStats.tsx";
import { ProductionBuilderDrawer } from "./ProductionBuilderDrawer.tsx";
import { SeatInfoProvider } from "../presentation/SeatInfoContext.tsx";
import type { PlayerView, PendingChoiceDto } from "../protocol/types.ts";

const player = (over: Partial<PlayerView>): PlayerView =>
  ({
    id: "seat_1",
    faction: "sol",
    technologies: [],
    leaders: {},
    ...over,
  }) as PlayerView;

const renderStats = (unit: string, over: Partial<PlayerView> = {}, price = {}) =>
  render(
    <SeatInfoProvider players={[player(over)]} viewerSeat="seat_1">
      <UnitBuildStats unit={unit} name={unit} price={price} />
    </SeatInfoProvider>,
  );

describe("UnitBuildStats", () => {
  it("shows cost, combat with hit chance and movement without hovering", () => {
    renderStats("dreadnought", { faction: "hacan", technologies: ["dn2"] }, { cost: 4 });
    const list = screen.getByRole("list", { name: "dreadnought stats" });
    expect(list).toHaveTextContent("Cost 4");
    expect(list).toHaveTextContent("Combat 5 · 60%");
    expect(list).toHaveTextContent("Move 2");
    expect(list).toHaveTextContent("Capacity 1");
    expect(list).toHaveTextContent("Sustain");
    expect(list).toHaveTextContent("Bombard 5 · 60%");
  });
  it("labels the combat value for screen readers", () => {
    renderStats("warsun", { faction: "hacan" }, { cost: 12 });
    expect(
      screen.getByLabelText("Combat: hits on 3 or higher, 80 percent per die, 3 dice"),
    ).toBeInTheDocument();
    expect(screen.getByTestId("unit-stats")).toHaveTextContent("Combat 3 ×3 · 80% per die");
  });
  it("a unit without combat shows no combat entry", () => {
    renderStats("spacedock", {}, { cost: 4 });
    expect(screen.getByTestId("unit-stats")).not.toHaveTextContent("Combat");
    expect(screen.getByTestId("unit-stats")).toHaveTextContent("Production +2");
  });
  it("uses the seat's own flagship and variant carrier", () => {
    const { unmount } = renderStats("flagship", { faction: "sol" }, { cost: 8 });
    expect(screen.getByTestId("unit-stats")).toHaveTextContent("Capacity 12");
    unmount();
    renderStats("carrier", { faction: "sol", technologies: ["ac2"] }, { cost: 3 });
    expect(screen.getByTestId("unit-stats")).toHaveTextContent("Move 2");
    expect(screen.getByTestId("unit-stats")).toHaveTextContent("Capacity 8");
  });
  it("shows the cost forms", () => {
    expect(costText({ cost: 1, count: 2 }, undefined)).toBe("2 for 1");
    expect(costText({}, { cost: 0.5 } as never)).toBe("2 for 1");
    expect(costText({ cost: 3 }, undefined)).toBe("3");
    expect(costText({ free: true, cost: 3 }, undefined)).toBe("Free");
    renderStats("cruiser", {}, { cost: 1, printedCost: 2 });
    expect(screen.getByText("2", { selector: "s" })).toBeInTheDocument();
  });
  it("an unknown unit shows only the offered cost", () => {
    renderStats("mystery_unit", {}, { cost: 5 });
    expect(screen.getByTestId("unit-stats")).toHaveTextContent("Cost 5");
    expect(screen.getByTestId("unit-stats")).not.toHaveTextContent("Combat");
  });
});

const offer = (id: string, unit: string, cost: number, count: number) => ({
  id,
  kind: "produce",
  label: `produce ${count}x ${unit} for ${cost}`,
  payload: { unit, cost, count, available_resources: 3 },
});

const choice = {
  actor: "seat_1",
  nonce: "n",
  prompt: "produce",
  context: { subtype: "produce_unit", outstanding: [{ amount: 3, paid: 0 }] },
  options: [
    offer("b|carrier", "carrier", 3, 1),
    offer("b|dreadnought", "dreadnought", 4, 1),
    offer("b|infantry", "infantry", 1, 2),
    { id: "decline", kind: "decline", label: "Done", payload: {} },
  ],
} as unknown as PendingChoiceDto;

describe("ProductionBuilderDrawer rows", () => {
  const renderDrawer = () =>
    render(
      <SeatInfoProvider players={[player({ technologies: ["ac2"] })]} viewerSeat="seat_1">
        <ProductionBuilderDrawer
          choice={choice}
          viewerSeat="seat_1"
          onSubmit={async () => {}}
          onClose={() => {}}
          isOpen
        />
      </SeatInfoProvider>,
    );
  it("each option shows its stats at rest, with the info button kept", () => {
    renderDrawer();
    const carrier = screen.getByTestId("produce-option-b|carrier");
    expect(carrier).toHaveTextContent("Advanced Carrier II");
    expect(carrier).toHaveTextContent("Combat 9 · 20%");
    expect(carrier).toHaveTextContent("Move 2");
    expect(
      within(carrier).getByRole("button", { name: "Advanced Carrier II: unit details" }),
    ).toBeInTheDocument();
    expect(screen.getByTestId("produce-option-b|infantry")).toHaveTextContent("2 for 1");
  });
  it("groups ships and ground forces", () => {
    renderDrawer();
    expect(screen.getByRole("heading", { name: "Ships" })).toBeInTheDocument();
    expect(screen.getByRole("heading", { name: "Ground forces" })).toBeInTheDocument();
    expect(
      within(screen.getByTestId("production-group-ground")).getByTestId("produce-option-b|infantry"),
    ).toBeInTheDocument();
  });
  it("says why an option cannot be added", () => {
    renderDrawer();
    const dreadnought = screen.getByTestId("produce-option-b|dreadnought");
    expect(dreadnought).toHaveTextContent("Needs 1 more");
    expect(screen.getByRole("button", { name: "Add 1x dreadnought for 4" })).toBeDisabled();
    expect(screen.getByTestId("produce-option-b|carrier")).not.toHaveTextContent("Needs");
    fireEvent.click(screen.getByRole("button", { name: "Add 1x carrier for 3" }));
    expect(screen.getByTestId("produce-option-b|infantry")).toHaveTextContent("Needs 1 more");
  });
});
