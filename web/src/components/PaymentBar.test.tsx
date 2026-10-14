import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { PaymentBar } from "./PaymentBar.tsx";
import {
  PaymentDraftProvider,
  usePaymentDraftState,
} from "../presentation/PaymentDraftContext.tsx";
import type { PendingChoiceDto } from "../protocol/types.ts";

const choice: PendingChoiceDto = {
  prompt: "pay 5 resources to produce",
  actor: "p1",
  nonce: "n1",
  context: { subtype: "pay_resources", outstanding: [{ kind: "resources", amount: 5, paid: 0 }] },
  options: [
    { id: "exhaust|jord", label: "Jord", kind: "pay", payload: { worth: 4, kind: "resources", planet_name: "Jord" } },
    { id: "exhaust|lodor", label: "Lodor", kind: "pay", payload: { worth: 3, kind: "resources", planet_name: "Lodor" } },
    { id: "exhaust|quann", label: "Quann", kind: "pay", payload: { worth: 1, kind: "resources", planet_name: "Quann" } },
    { id: "decline", label: "Cancel", kind: "decline" },
  ],
};

function Harness(props: {
  onSubmitBatch?: (plan: unknown) => Promise<void>;
  stage?: string[];
  viewer?: string;
}) {
  const api = usePaymentDraftState(choice.nonce);
  return (
    <PaymentDraftProvider value={api}>
      <button data-testid="stage-jord" onClick={() => api.togglePlanet("exhaust|jord")} />
      <PaymentBar
        choice={choice}
        viewerSeat={props.viewer ?? "p1"}
        onSubmit={vi.fn()}
        onSubmitBatch={props.onSubmitBatch as never}
        onOpenList={vi.fn()}
      />
    </PaymentDraftProvider>
  );
}

describe("PaymentBar", () => {
  it("shows paid against owed, the remainder and an invalid-state message", () => {
    render(<Harness />);
    expect(screen.getByTestId("payment-bar-staged")).toHaveTextContent("0");
    expect(screen.getByTestId("payment-bar-remaining")).toHaveTextContent("5 remaining");
    expect(screen.getByTestId("payment-bar-problem")).toHaveTextContent(/Short by 5/);
    expect(screen.getByTestId("confirm-payment-btn")).toBeDisabled();
    fireEvent.click(screen.getByTestId("stage-jord"));
    expect(screen.getByTestId("payment-bar-staged")).toHaveTextContent("4");
    expect(screen.getByTestId("payment-bar-remaining")).toHaveTextContent("1 remaining");
  });

  it("auto-pay stages a covering selection, and confirm submits the basket plan", async () => {
    const onSubmitBatch = vi.fn().mockResolvedValue(undefined);
    render(<Harness onSubmitBatch={onSubmitBatch} />);
    fireEvent.click(screen.getByTestId("auto-pay-btn"));
    expect(screen.getByTestId("payment-bar-remaining")).toHaveTextContent("Covered");
    fireEvent.click(screen.getByTestId("confirm-payment-btn"));
    await waitFor(() => expect(onSubmitBatch).toHaveBeenCalledTimes(1));
    const plan = onSubmitBatch.mock.calls[0][0];
    expect(plan.kind).toBe("payment");
    expect(plan.steps).toEqual([
      { kind: "exhaust", planet: "jord" },
      { kind: "exhaust", planet: "quann" },
    ]);
  });

  it("renders nothing for another seat", () => {
    render(<Harness viewer="p2" />);
    expect(screen.queryByTestId("payment-bar")).not.toBeInTheDocument();
  });
});
