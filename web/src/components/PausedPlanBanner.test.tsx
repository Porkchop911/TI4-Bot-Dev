import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PausedPlanBanner } from "./PausedPlanBanner.tsx";
import type { BatchResume } from "../protocol/client.ts";
import type { PendingChoiceDto } from "../protocol/types.ts";

const resume: BatchResume = {
  plan: {
    kind: "tactical_movement",
    destination: "22",
    steps: [
      { kind: "move", origin: "23", unit: "carrier", damaged: false },
      { kind: "done_moving" },
    ],
  },
  applied: 1,
  waiting: { subtype: "reaction_after_SHIP_MOVED", ownSeat: false },
};
const pending = (subtype: string): PendingChoiceDto =>
  ({
    actor: "seat_1",
    nonce: "n",
    prompt: "p",
    options: [],
    context: { subtype },
  }) as unknown as PendingChoiceDto;

describe("PausedPlanBanner", () => {
  it("waits while a reaction is pending and offers no continue button", () => {
    render(
      <PausedPlanBanner
        resume={resume}
        choice={pending("reaction_after_SHIP_MOVED")}
        viewerSeat="seat_1"
        onContinue={vi.fn()}
        onDismiss={vi.fn()}
      />,
    );
    expect(screen.getByTestId("paused-plan").textContent).toMatch(/paused after 1 step/);
    expect(screen.queryByText("Continue plan")).toBeNull();
  });

  it("continues the plan once the seat is back at a movement step", () => {
    const onContinue = vi.fn().mockResolvedValue(undefined);
    render(
      <PausedPlanBanner
        resume={resume}
        choice={pending("movement_step")}
        viewerSeat="seat_1"
        onContinue={onContinue}
        onDismiss={vi.fn()}
      />,
    );
    expect(screen.getByTestId("paused-plan").textContent).toMatch(/2 steps still to do/);
    fireEvent.click(screen.getByText("Continue plan"));
    expect(onContinue).toHaveBeenCalledTimes(1);
  });
});
