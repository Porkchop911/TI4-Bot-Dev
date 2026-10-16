import React from "react";
import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import type { ChoiceOption, PendingChoiceDto } from "../protocol/types.ts";
import { ReadyPlanetsBar } from "./ReadyPlanetsBar.tsx";
import {
  ReadyPlanetsDraftProvider,
  useReadyPlanetsDraft,
} from "../presentation/ReadyPlanetsDraftContext.tsx";

const choice = (...ids: string[]): PendingChoiceDto => ({
  actor: "b",
  nonce: "n1",
  prompt: "ready which planet",
  options: ids.map((id): ChoiceOption => ({ id, kind: "ready", label: `ready ${id}`, payload: {} })),
  context: { subtype: "ready_planet" } as PendingChoiceDto["context"],
});

const click = (id: string) => fireEvent.click(screen.getByTestId(id));
const confirmButton = () => screen.getByTestId("ready-bar-confirm") as HTMLButtonElement;

const Shared: React.FC<{ pending: PendingChoiceDto; onConfirm: (plan: unknown) => Promise<void>; viewer?: string }> = ({
  pending,
  onConfirm,
  viewer = "b",
}) => {
  const draft = useReadyPlanetsDraft(pending);
  return (
    <ReadyPlanetsDraftProvider value={draft}>
      <button type="button" data-testid="map-tap-quann" onClick={() => draft.toggle("quann")} />
      <ReadyPlanetsBar choice={pending} viewerSeat={viewer} onConfirm={onConfirm} />
    </ReadyPlanetsDraftProvider>
  );
};

describe("ReadyPlanetsBar", () => {
  it("picks two planets from the list and sends one batch with a single Confirm", async () => {
    const onConfirm = vi.fn().mockResolvedValue(undefined);
    render(<Shared pending={choice("jord", "lodor", "quann")} onConfirm={onConfirm} />);
    expect(confirmButton().disabled).toBe(true);
    expect(screen.getByTestId("ready-bar-count").textContent).toBe("0 of 2 picked");
    click("ready-planet-chip-lodor");
    expect(confirmButton().disabled).toBe(true);
    click("ready-planet-chip-jord");
    expect(screen.getByTestId("ready-bar-count").textContent).toBe("2 of 2 picked");
    expect(confirmButton().disabled).toBe(false);
    await act(async () => {
      click("ready-bar-confirm");
    });
    expect(onConfirm).toHaveBeenCalledTimes(1);
    expect(onConfirm).toHaveBeenCalledWith({
      kind: "ready_planets",
      steps: [
        { kind: "ready_planet", planet: "lodor" },
        { kind: "ready_planet", planet: "jord" },
      ],
    });
  });

  it("shares the picks with a tap on the map", () => {
    render(<Shared pending={choice("jord", "lodor", "quann")} onConfirm={vi.fn()} />);
    click("map-tap-quann");
    expect(screen.getByTestId("ready-planet-chip-quann").getAttribute("aria-pressed")).toBe("true");
    click("ready-planet-chip-quann");
    expect(screen.getByTestId("ready-planet-chip-quann").getAttribute("aria-pressed")).toBe("false");
  });

  it("starts with both planets picked when there is nothing to choose", () => {
    render(<Shared pending={choice("jord", "lodor")} onConfirm={vi.fn()} />);
    expect(confirmButton().disabled).toBe(false);
    expect(screen.getByTestId("ready-bar-count").textContent).toBe("2 of 2 picked");
  });

  it("owes one planet when only one is exhausted", () => {
    render(<Shared pending={choice("jord")} onConfirm={vi.fn()} />);
    expect(screen.getByTestId("ready-bar-prompt").textContent).toBe("Ready this planet");
    expect(confirmButton().disabled).toBe(false);
  });

  it("folds down to the summary and Confirm, keeping the picks", () => {
    render(<Shared pending={choice("jord", "lodor", "quann")} onConfirm={vi.fn()} />);
    click("ready-planet-chip-jord");
    click("ready-bar-collapse");
    expect(screen.queryByTestId("ready-bar-chips")).toBeNull();
    expect(screen.getByTestId("ready-bar-confirm")).toBeInTheDocument();
    expect(screen.getByTestId("ready-bar-count").textContent).toBe("1 of 2 picked");
    click("ready-bar-collapse");
    expect(screen.getByTestId("ready-planet-chip-jord").getAttribute("aria-pressed")).toBe("true");
  });

  it("is read-only for a seat that is not deciding", () => {
    render(<Shared pending={choice("jord", "lodor")} onConfirm={vi.fn()} viewer="c" />);
    expect(confirmButton().disabled).toBe(true);
    expect((screen.getByTestId("ready-planet-chip-jord") as HTMLButtonElement).disabled).toBe(true);
  });

  it("shows a refusal and starts over", async () => {
    const onConfirm = vi.fn().mockRejectedValue(new Error("The planet is no longer exhausted"));
    render(<Shared pending={choice("jord", "lodor", "quann")} onConfirm={onConfirm} />);
    click("ready-planet-chip-jord");
    click("ready-planet-chip-lodor");
    await act(async () => {
      click("ready-bar-confirm");
    });
    expect(screen.getByTestId("ready-bar-error").textContent).toMatch(/no longer exhausted/);
    expect(screen.getByTestId("ready-bar-count").textContent).toBe("0 of 2 picked");
  });
});
