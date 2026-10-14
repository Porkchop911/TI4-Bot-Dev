import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { PendingChoiceModal } from "./PendingChoiceModal.tsx";
import type { PendingChoiceDto } from "../protocol/types.ts";

const choice: PendingChoiceDto = {
  prompt: "let another player replenish commodities",
  actor: "a",
  nonce: "n1",
  context: { subtype: "trade_choose_replenish" } as never,
  options: [
    { id: "hacan", kind: "replenish", label: "hacan replenishes commodities" },
    { id: "done", kind: "decline", label: "nobody else replenishes" },
  ],
  details: {
    seats: { hacan: "seat_b" },
    commodities: { hacan: { have: 1, max: 6 } },
  },
};

describe("Trade replenish panel", () => {
  it("shows have/max → max per seat and submits the chosen seat", () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<PendingChoiceModal choice={choice} onSubmit={onSubmit} />);
    expect(screen.getByTestId("trade-replenish-figures")).toHaveTextContent("1/6 → 6 (+5)");
    fireEvent.click(screen.getByTestId("trade-replenish-hacan"));
    expect(onSubmit).toHaveBeenCalledWith("hacan");
  });

  it("Done submits the decline option", () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<PendingChoiceModal choice={choice} onSubmit={onSubmit} />);
    fireEvent.click(screen.getByTestId("trade-replenish-done"));
    expect(onSubmit).toHaveBeenCalledWith("done");
  });

  it("keeps the plain option list when the engine sent no commodities", () => {
    const plain = { ...choice, details: undefined };
    render(<PendingChoiceModal choice={plain} onSubmit={vi.fn()} />);
    expect(screen.queryByTestId("trade-replenish-panel")).toBeNull();
    expect(screen.getAllByTestId("choice-option")).toHaveLength(2);
  });
});
