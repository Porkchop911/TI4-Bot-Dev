import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { productionTotal } from "../presentation/productionDraft.ts";
import { productionTotalText } from "../presentation/productionTotalText.ts";
import { ProductionBuilderDrawer } from "./ProductionBuilderDrawer.tsx";

const offer = (unit: string, cost: number, printed = cost, extra: Record<string, unknown> = {}): ChoiceOptionDto => ({
  id: `build|${unit}|1`,
  kind: "produce",
  label: `produce ${unit}`,
  payload: { unit, cost, printed_cost: printed, discount: printed - cost, count: 1, production_spent: 1, available_resources: 9, ...extra },
});

describe("productionTotal", () => {
  const cruiser = offer("cruiser", 2);
  const carrier = offer("carrier", 3);
  it("sums several builds and counts them", () => {
    const t = productionTotal([cruiser, carrier], { [cruiser.id]: 2, [carrier.id]: 1 });
    expect(t).toMatchObject({ builds: 3, total: 7, printed: 7, discount: 0 });
    expect(t.lines.map((l) => [l.unit, l.batches, l.printed])).toEqual([["cruiser", 2, 4], ["carrier", 1, 3]]);
  });
  it("takes the one-time discount off once, not per build", () => {
    const a = offer("cruiser", 1, 2);
    const t = productionTotal([a], { [a.id]: 3 });
    expect(t).toMatchObject({ builds: 3, printed: 6, total: 5, discount: 1 });
  });
  it("charges nothing for an Amalgamation exchange but counts it as a build", () => {
    const ex = offer("carrier", 0, 3, { exchange: true });
    const t = productionTotal([carrier, ex], { [carrier.id]: 1, [ex.id]: 1 });
    expect(t).toMatchObject({ builds: 2, total: 3, printed: 3 });
    expect(t.lines[1].exchange).toBe(true);
  });
  it("is empty for an empty draft and null when a cost is unusable", () => {
    expect(productionTotal([cruiser], {})).toMatchObject({ builds: 0, total: 0 });
    const broken = { ...cruiser, payload: { unit: "cruiser", count: 1 } } as ChoiceOptionDto;
    expect(productionTotal([broken], { [broken.id]: 1 }).total).toBeNull();
  });
  it("words the total, marking a partial one", () => {
    expect(productionTotalText(7, 3)).toBe("Total: 7 resources — 3 builds");
    expect(productionTotalText(1, 1)).toBe("Total: 1 resource — 1 build");
    expect(productionTotalText(4, 2, true)).toBe("Total so far: 4 resources — 2 builds");
  });
});

const choice = (paid = 0): PendingChoiceDto => ({
  actor: "seat_1",
  nonce: "t",
  prompt: "produce",
  context: { subtype: "produce_unit", target: { System: "18" }, outstanding: [{ amount: 6, paid }] },
  options: [offer("cruiser", 2), offer("carrier", 3), { id: "done_producing", kind: "decline", label: "Done", payload: {} }],
});

const renderDrawer = (c: PendingChoiceDto) =>
  render(<ProductionBuilderDrawer choice={c} viewerSeat="seat_1" onSubmit={vi.fn()} onClose={vi.fn()} isOpen onQueueProduction={vi.fn()} />);

describe("builder total", () => {
  it("shows the live total, the comparison with what is available and the cost on the confirm button", () => {
    renderDrawer(choice());
    expect(screen.queryByTestId("production-total")).toBeNull();
    fireEvent.click(screen.getByTestId("produce-unit-btn-build|cruiser|1"));
    fireEvent.click(screen.getByTestId("produce-unit-btn-build|cruiser|1"));
    fireEvent.click(screen.getByTestId("produce-unit-btn-build|carrier|1"));
    expect(screen.getByTestId("production-total-sum")).toHaveTextContent("Total: 7 resources — 3 builds");
    expect(screen.getByTestId("production-total-available")).toHaveTextContent("You have 9 available · 2 left");
    expect(screen.getByTestId("production-total-lines")).toHaveTextContent("2× Cruiser4");
    // The accessible name stays what the harness clicks; the visible label carries the cost.
    const confirm = screen.getByRole("button", { name: "Confirm builds" });
    expect(confirm).toHaveTextContent("Build (7)");
    fireEvent.click(screen.getByLabelText("Remove carrier"));
    expect(screen.getByTestId("production-total-sum")).toHaveTextContent("Total: 4 resources — 2 builds");
    expect(confirm).toHaveTextContent("Build (4)");
  });
  it("marks the total as partial once earlier builds were already sent", () => {
    renderDrawer(choice(2));
    fireEvent.click(screen.getByTestId("produce-unit-btn-build|cruiser|1"));
    expect(screen.getByTestId("production-total-sum")).toHaveTextContent("Total so far: 2 resources — 1 build");
  });
});
