import React from "react";
import { describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import { galleryBoard } from "../dev/galleryBoard.ts";
import { actor } from "../dev/decisionGalleryCases.ts";
import { transitChoice, transitOption } from "../presentation/transitDiodes.fixtures.ts";
import { PlanetSelectionBar } from "./PlanetSelectionBar.tsx";

const options = [
  transitOption("18", "jord", "infantry", "18", "exhausted"),
  transitOption("18", "jord", "infantry", "19", "wellon"),
  transitOption("18", "jord", "infantry", "22", "tarmann"),
  transitOption("22", "tarmann", "infantry", "18", "jord"),
  transitOption("22", "tarmann", "infantry", "19", "wellon"),
];
const choice = transitChoice(options);
const click = (id: string) => fireEvent.click(screen.getByTestId(id));
const confirm = () => screen.getByTestId("transit-confirm") as HTMLButtonElement;

function renderBar(props: Partial<React.ComponentProps<typeof PlanetSelectionBar>> = {}) {
  const onSubmit = vi.fn().mockResolvedValue(undefined);
  const utils = render(
    <PlanetSelectionBar choice={choice} viewerSeat={actor} boardView={galleryBoard} onSubmit={onSubmit} {...props} />,
  );
  return { onSubmit, ...utils };
}

describe("TransitDiodesPanel (via PlanetSelectionBar)", () => {
  it("replaces the planet chips with the stepwise panel", () => {
    renderBar();
    expect(screen.getByTestId("transit-panel").dataset.step).toBe("1");
    expect(screen.queryByTestId("planet-selection-candidates")).toBeNull();
    expect(screen.getByTestId("transit-source-system-18").textContent).toContain("#18 [Mecatol Rex]");
    expect(screen.getByTestId("transit-moves-left").textContent).toBe("3 moves left");
    expect(confirm().disabled).toBe(true);
  });

  it("picks the force, then the destination, and submits the engine's option id", async () => {
    const { onSubmit } = renderBar();
    click("transit-force-22|tarmann|infantry");
    expect(screen.getByTestId("transit-panel").dataset.step).toBe("2");
    expect(screen.getByTestId("transit-picked").textContent).toContain("Infantry from Tar'Mann in #22 [Tar'Mann]");
    // Only Tar'Mann's destinations are offered.
    expect(screen.queryByTestId("transit-destination-tarmann")).toBeNull();
    expect(screen.queryByTestId("transit-destination-exhausted")).toBeNull();
    expect(screen.getByTestId("transit-destination-jord").textContent).toContain("Here now: 1 infantry");
    expect(confirm().disabled).toBe(true);
    click("transit-destination-wellon");
    expect(screen.getByTestId("transit-summary").textContent).toBe(
      "Move 1 Infantry: Tar'Mann (#22 [Tar'Mann]) → Wellon (#19 [Wellon])",
    );
    await act(async () => {
      fireEvent.click(confirm());
    });
    expect(onSubmit).toHaveBeenCalledWith("transit|22|tarmann|infantry|19|wellon");
  });

  it("Back and Change return to step 1 and clear the destination", () => {
    const onSelectOption = vi.fn();
    renderBar({ onSelectOption });
    click("transit-force-18|jord|infantry");
    click("transit-destination-wellon");
    expect(onSelectOption).toHaveBeenLastCalledWith("transit|18|jord|infantry|19|wellon");
    click("transit-back");
    expect(screen.getByTestId("transit-panel").dataset.step).toBe("1");
    expect(onSelectOption).toHaveBeenLastCalledWith("");
    click("transit-force-18|jord|infantry");
    click("transit-change-source");
    expect(screen.getByTestId("transit-panel").dataset.step).toBe("1");
  });

  it("keeps Finish redeployment, which sends the decline", async () => {
    const { onSubmit } = renderBar();
    expect(screen.getByTestId("transit-decline").textContent).toBe("Finish redeployment");
    await act(async () => {
      click("transit-decline");
    });
    expect(onSubmit).toHaveBeenCalledWith("decline");
  });

  it("opens on a prepared move and follows map taps", () => {
    const { rerender, onSubmit } = renderBar({ selectedOptionId: "transit|18|jord|infantry|22|tarmann" });
    expect(screen.getByTestId("transit-panel").dataset.step).toBe("3");
    expect(screen.getByTestId("transit-destination-tarmann").getAttribute("aria-pressed")).toBe("true");
    // A map tap on another destination of the same force moves the pick there.
    rerender(
      <PlanetSelectionBar
        choice={choice}
        viewerSeat={actor}
        boardView={galleryBoard}
        onSubmit={onSubmit}
        selectedOptionId="transit|18|jord|infantry|22|tarmann"
        selectedPlanetId="wellon"
      />,
    );
    expect(screen.getByTestId("transit-summary").textContent).toContain("→ Wellon");
  });

  it("a map tap on a planet with one kind of ground force picks that force", () => {
    const { rerender, onSubmit } = renderBar({ selectedPlanetId: null });
    rerender(
      <PlanetSelectionBar choice={choice} viewerSeat={actor} boardView={galleryBoard} onSubmit={onSubmit} selectedPlanetId="tarmann" />,
    );
    expect(screen.getByTestId("transit-panel").dataset.step).toBe("2");
    expect(screen.getByTestId("transit-picked").textContent).toContain("Tar'Mann");
  });

  it("shows the waiting line to other players", () => {
    renderBar({ viewerSeat: "other_seat" });
    expect(screen.queryByTestId("transit-panel")).toBeNull();
    expect(screen.getByTestId("planet-selection-bar").textContent).toContain("Waiting for");
  });
});
