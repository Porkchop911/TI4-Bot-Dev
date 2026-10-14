import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { GameShell } from "./GameShell.tsx";
import type { BasketPlan } from "../protocol/client.ts";
import type { PendingChoiceDto } from "../protocol/types.ts";
import { SINGLE_PAYMENT_KEY } from "../hooks/useSinglePaymentSetting.ts";

const produce = (nonce: string): PendingChoiceDto => ({
  actor: "p1",
  nonce,
  prompt: "produce in 18",
  context: {
    subtype: "produce_unit",
    target: { System: "18" },
    outstanding: [{ amount: 3, paid: 0 }],
  },
  options: [
    {
      id: "build|destroyer|1",
      kind: "produce",
      label: "Destroyer",
      payload: {
        unit: "destroyer",
        production_spent: 1,
        cost: 2,
        printed_cost: 2,
        available_resources: 8,
      },
    },
    { id: "done_producing", kind: "decline", label: "Done" },
  ],
});

const planet = (id: string, name: string, worth: number) => ({
  id: `exhaust|${id}`,
  kind: "pay",
  label: name,
  payload: { worth, planet_name: name },
});

/** The engine's payment question for one build: `owed` still due, the planets offered. */
const pay = (nonce: string, owed: number, planets: ReturnType<typeof planet>[]): PendingChoiceDto => ({
  actor: "p1",
  nonce,
  prompt: `Pay ${owed}`,
  context: {
    subtype: "pay_resources",
    target: { System: "18" },
    outstanding: [{ kind: "Resources", amount: owed, paid: 0 }],
  },
  options: [...planets, { id: "trade_good", kind: "pay", label: "Trade good", payload: { worth: 1 } }],
});

const place = (nonce: string): PendingChoiceDto => ({
  actor: "p1",
  nonce,
  prompt: "place the unit",
  context: { subtype: "place_unit", target: { System: "18" } },
  options: [{ id: "place|space", kind: "place", label: "Space" }],
});

const JORD = planet("jord", "Jord", 3);
const VEGA = planet("vega", "Vega", 3);
const LODOR = planet("lodor", "Lodor", 2);

function setup() {
  const onSubmit = vi.fn().mockResolvedValue(undefined);
  const onBatch = vi.fn<(plan: BasketPlan) => Promise<void>>().mockResolvedValue(undefined);
  const view = (choice: PendingChoiceDto | null) => (
    <GameShell
      header={null}
      board={null}
      playerSheet={null}
      events={[]}
      choice={choice}
      viewerSeat="p1"
      onSubmitChoice={onSubmit}
      onSubmitBasketBatch={onBatch}
    />
  );
  return { onSubmit, onBatch, view };
}

/** Stages `count` destroyers in the builder and confirms. */
function stageAndConfirm(count: number) {
  for (let i = 0; i < count; i++) fireEvent.click(screen.getByTestId("produce-unit-btn-build|destroyer|1"));
  fireEvent.click(screen.getByRole("button", { name: "Confirm builds" }));
}

async function confirmPanel() {
  await waitFor(() => expect(screen.getByTestId("confirm-payment-btn")).toBeEnabled());
  fireEvent.click(screen.getByTestId("confirm-payment-btn"));
}

const payments = (onBatch: ReturnType<typeof setup>["onBatch"]) =>
  onBatch.mock.calls.map(([plan]) => plan).filter((plan) => plan.kind === "payment");

describe("one payment for a whole production", () => {
  beforeEach(() => localStorage.removeItem(SINGLE_PAYMENT_KEY));
  afterEach(() => localStorage.removeItem(SINGLE_PAYMENT_KEY));

  it("asks once for 3 builds, then pays the later builds from the plan without a prompt", async () => {
    const { onBatch, view } = setup();
    const { rerender } = render(view(produce("p-1")));
    stageAndConfirm(3);
    await waitFor(() => expect(onBatch).toHaveBeenCalledTimes(1));
    expect(onBatch.mock.calls[0][0]).toMatchObject({ kind: "production", steps: [{ unit: "destroyer" }] });

    // Build 1: the engine asks for 2; the panel is for all three builds (total 6).
    rerender(view(pay("q-1", 2, [JORD, VEGA, LODOR])));
    expect(screen.getByTestId("payment-drawer-title")).toHaveTextContent("Pay for 3 units: total");
    await waitFor(() => expect(screen.getByTestId("committed-amount")).toHaveTextContent("6"));
    expect(screen.getByTestId("confirm-payment-btn")).toBeEnabled();
    await confirmPanel();
    await waitFor(() => expect(payments(onBatch)).toHaveLength(1));
    // Only this question's part is sent: one planet covers 2 and leaves 1 of credit.
    expect(payments(onBatch)[0]).toEqual({ kind: "payment", steps: [{ kind: "exhaust", planet: "jord" }] });

    rerender(view(place("pl-1")));
    rerender(view(produce("p-2")));
    await waitFor(() => expect(onBatch).toHaveBeenCalledTimes(3));

    // Build 2: owed 1 after the credit. No panel; the plan answers.
    rerender(view(pay("q-2", 1, [VEGA, LODOR])));
    expect(screen.queryByTestId("payment-drawer")).toBeNull();
    expect(screen.getByTestId("production-pay-auto")).toBeInTheDocument();
    await waitFor(() => expect(payments(onBatch)).toHaveLength(2));
    expect(payments(onBatch)[1]).toEqual({ kind: "payment", steps: [{ kind: "exhaust", planet: "vega" }] });

    // Build 3 is covered by the credit the engine kept: placement follows directly.
    rerender(view(place("pl-2")));
    rerender(view(produce("p-3")));
    await waitFor(() => expect(onBatch).toHaveBeenCalledTimes(5));
    expect(payments(onBatch)).toHaveLength(2);
  });

  it("stops at an interruption, shows it, and resumes at the next matching payment", async () => {
    const { onBatch, view } = setup();
    const { rerender } = render(view(produce("p-1")));
    stageAndConfirm(2);
    await waitFor(() => expect(onBatch).toHaveBeenCalledTimes(1));
    rerender(view(pay("q-1", 2, [JORD, VEGA, LODOR])));
    await confirmPanel();
    await waitFor(() => expect(payments(onBatch)).toHaveLength(1));
    rerender(view(place("pl-1")));
    rerender(view(produce("p-2")));
    await waitFor(() => expect(onBatch).toHaveBeenCalledTimes(3));

    // Another seat's reaction window interrupts: nothing is paid, the window is shown as usual.
    const reaction: PendingChoiceDto = {
      actor: "p2",
      nonce: "r-1",
      prompt: "when a unit is produced",
      context: { subtype: "reaction_after_UNITS_PRODUCED" },
      options: [{ id: "decline", kind: "decline", label: "Decline" }],
    };
    rerender(view(reaction));
    await act(async () => {});
    expect(payments(onBatch)).toHaveLength(1);

    // The payment that follows still matches the plan: answered without a prompt.
    rerender(view(pay("q-2", 1, [VEGA, LODOR])));
    await waitFor(() => expect(payments(onBatch)).toHaveLength(2));
    expect(screen.queryByTestId("payment-drawer")).toBeNull();
  });

  it("asks normally, with the suggestion staged, when the question does not match the plan", async () => {
    const { onBatch, view } = setup();
    const { rerender } = render(view(produce("p-1")));
    stageAndConfirm(2);
    await waitFor(() => expect(onBatch).toHaveBeenCalledTimes(1));
    rerender(view(pay("q-1", 2, [JORD, VEGA, LODOR])));
    await confirmPanel();
    await waitFor(() => expect(payments(onBatch)).toHaveLength(1));
    rerender(view(place("pl-1")));
    rerender(view(produce("p-2")));
    await waitFor(() => expect(onBatch).toHaveBeenCalledTimes(3));

    // None of the planned planets is offered any more (only Mirage).
    rerender(view(pay("q-2", 1, [planet("mirage", "Mirage", 1)])));
    await waitFor(() => expect(screen.getByTestId("payment-drawer")).toBeInTheDocument());
    expect(payments(onBatch)).toHaveLength(1);
    expect(screen.getByTestId("production-pay-note")).toBeInTheDocument();
    await waitFor(() => expect(screen.getByTestId("committed-amount")).toHaveTextContent("1"));
    expect(screen.getByTestId("planet-card-exhaust|mirage").querySelector("input")).toBeChecked();
  });

  it("stops when an automatic payment is rejected and asks by hand", async () => {
    const { onBatch, view } = setup();
    const { rerender } = render(view(produce("p-1")));
    stageAndConfirm(2);
    await waitFor(() => expect(onBatch).toHaveBeenCalledTimes(1));
    rerender(view(pay("q-1", 2, [JORD, VEGA, LODOR])));
    await confirmPanel();
    await waitFor(() => expect(payments(onBatch)).toHaveLength(1));
    rerender(view(place("pl-1")));
    rerender(view(produce("p-2")));
    await waitFor(() => expect(onBatch).toHaveBeenCalledTimes(3));

    onBatch.mockRejectedValueOnce(new Error("that planet is exhausted"));
    rerender(view(pay("q-2", 1, [VEGA, LODOR])));
    await waitFor(() => expect(screen.getByTestId("production-pay-note")).toHaveTextContent(/that planet is exhausted/));
    expect(payments(onBatch)).toHaveLength(2);
    expect(screen.getByTestId("payment-drawer")).toBeInTheDocument();
  });

  it("asks per build again when the setting is off", async () => {
    localStorage.setItem(SINGLE_PAYMENT_KEY, "false");
    const { onBatch, view } = setup();
    const { rerender } = render(view(produce("p-1")));
    stageAndConfirm(2);
    await waitFor(() => expect(onBatch).toHaveBeenCalledTimes(1));
    rerender(view(pay("q-1", 2, [JORD, VEGA, LODOR])));
    expect(screen.getByTestId("payment-drawer-title")).not.toHaveTextContent("units");
    expect(screen.getByTestId("committed-amount")).toHaveTextContent("0");
    fireEvent.click(screen.getByTestId("auto-pay-btn"));
    await confirmPanel();
    await waitFor(() => expect(payments(onBatch)).toHaveLength(1));
    rerender(view(place("pl-1")));
    rerender(view(produce("p-2")));
    await waitFor(() => expect(onBatch).toHaveBeenCalledTimes(3));
    rerender(view(pay("q-2", 1, [VEGA, LODOR])));
    expect(screen.getByTestId("payment-drawer")).toBeInTheDocument();
    await act(async () => {});
    expect(payments(onBatch)).toHaveLength(1);
  });

  it("'Ask for each build' pays this build only and asks again for the next", async () => {
    const { onBatch, view } = setup();
    const { rerender } = render(view(produce("p-1")));
    stageAndConfirm(2);
    await waitFor(() => expect(onBatch).toHaveBeenCalledTimes(1));
    rerender(view(pay("q-1", 2, [JORD, VEGA, LODOR])));
    fireEvent.click(screen.getByTestId("ask-each-payment-btn"));
    await waitFor(() => expect(screen.getByTestId("payment-drawer-title")).not.toHaveTextContent("units"));
    await waitFor(() => expect(screen.getByTestId("confirm-payment-btn")).toBeEnabled());
    await confirmPanel();
    await waitFor(() => expect(payments(onBatch)).toHaveLength(1));
    rerender(view(place("pl-1")));
    rerender(view(produce("p-2")));
    await waitFor(() => expect(onBatch).toHaveBeenCalledTimes(3));
    rerender(view(pay("q-2", 1, [VEGA, LODOR])));
    expect(screen.getByTestId("payment-drawer")).toBeInTheDocument();
    await act(async () => {});
    expect(payments(onBatch)).toHaveLength(1);
  });
});
