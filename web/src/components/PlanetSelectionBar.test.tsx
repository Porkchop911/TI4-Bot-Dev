import { useState } from "react";
import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { PlanetSelectionBar } from "./PlanetSelectionBar.tsx";
import { BoardView, PendingChoiceDto } from "../protocol/types.ts";

const board: BoardView = {
  systems: {
    "26": {
      system_id: "26",
      command_tokens: [],
      units: [
        { unit_type: "infantry", owner: "p1", planet: "lodor", damaged: false },
        { unit_type: "infantry", owner: "p1", planet: "lodor", damaged: false },
        { unit_type: "pds", owner: "p1", planet: "lodor", damaged: false },
      ],
      planets: { lodor: { planet_id: "lodor", controlled_by: "p1", exhausted: false } },
    },
  },
  map_tiles: [
    {
      system_id: "26",
      label: "Lodor",
      q: 0,
      r: 0,
      planets: [{ id: "lodor", label: "Lodor", resources: 3, influence: 1, traits: ["cultural"] }],
    },
    {
      system_id: "30",
      label: "Centauri / Gral",
      q: 1,
      r: 0,
      planets: [
        { id: "gral", label: "Gral", resources: 1, influence: 1, tech_specialties: ["propulsion"] },
      ],
    },
  ],
};

const mining: PendingChoiceDto = {
  nonce: "n1",
  actor: "p1",
  prompt: "Mining Initiative: mine which planet",
  context: {
    subtype: "mining_initiative_pick_planet",
    source: { ActionCard: "mining_initiative" },
  },
  options: [
    { id: "lodor", kind: "planet", label: "Lodor", payload: { planet: "lodor", system: "26" } },
    { id: "gral", kind: "planet", label: "Gral", payload: { planet: "gral" } },
  ],
};

const structures: PendingChoiceDto = {
  nonce: "n2",
  actor: "p1",
  prompt: "place a structure",
  context: { subtype: "place_structure", source: { Content: "place_structure" } },
  options: [
    {
      id: "pds|26|lodor",
      kind: "build",
      label: "place pds on lodor",
      payload: { planet: "lodor", system: "26", unit: "pds" },
    },
    {
      id: "spacedock|26|lodor",
      kind: "build",
      label: "place spacedock on lodor",
      payload: { planet: "lodor", system: "26", unit: "spacedock" },
    },
    { id: "decline", kind: "decline", label: "decline" },
  ],
};

const bioStims: PendingChoiceDto = {
  nonce: "n3",
  actor: "p1",
  prompt: "Bio-Stims",
  context: { subtype: "bio_stims_ready", source: { Content: "bs" } },
  options: [
    {
      id: "ready|planet|gral",
      kind: "ready",
      label: "Bio-Stims: ready gral",
      payload: { planet: "gral", technology: "bs" },
    },
    {
      id: "ready|technology|st",
      kind: "ready_technology",
      label: "Bio-Stims: ready Sarween Tools",
      payload: { technology: "st", bio_stims: true },
    },
    { id: "decline", kind: "decline", label: "decline" },
  ],
};

/** Hosts the bar with lifted selection state, like App/GameShell do. */
function Host({
  choice,
  onSubmit,
  initialOption,
  initialPlanet = null,
  lastError,
}: {
  choice: PendingChoiceDto;
  onSubmit: (id: string) => Promise<void>;
  initialOption?: string;
  initialPlanet?: string | null;
  lastError?: string | null;
}) {
  const [optionId, setOptionId] = useState<string | undefined>(initialOption);
  const [planetId, setPlanetId] = useState<string | null>(initialPlanet);
  return (
    <PlanetSelectionBar
      choice={choice}
      viewerSeat="p1"
      selectedOptionId={optionId}
      selectedPlanetId={planetId}
      onSelectOption={(id) => setOptionId(id || undefined)}
      onSelectPlanet={setPlanetId}
      onSubmit={onSubmit}
      boardView={board}
      lastError={lastError}
    />
  );
}

describe("PlanetSelectionBar", () => {
  it("shows the source and the action while selecting, with candidate chips", () => {
    render(<Host choice={mining} onSubmit={vi.fn()} />);
    expect(screen.getByTestId("planet-selection-source")).toHaveTextContent("Mining Initiative");
    expect(screen.getByTestId("planet-selection-prompt")).toHaveTextContent("Mine which planet");
    expect(screen.getByText(/Click a highlighted planet/)).toBeInTheDocument();
    expect(screen.getByTestId("planet-chip-lodor")).toHaveTextContent("Lodor");
    expect(within(screen.getByTestId("planet-chip-lodor")).getByLabelText("3 resources")).toBeInTheDocument();
    expect(within(screen.getByTestId("planet-chip-lodor")).getByLabelText("1 influence")).toBeInTheDocument();
    expect(screen.getByTestId("planet-chip-gral")).toBeInTheDocument();
    expect(screen.queryByTestId("confirm-planet-btn")).not.toBeInTheDocument();
    expect(screen.queryByTestId("decline-planet-btn")).not.toBeInTheDocument();
  });

  it("states the concrete action with planet stats and submits the selected option", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<Host choice={mining} onSubmit={onSubmit} initialOption="lodor" />);
    const action = screen.getByTestId("planet-selection-action");
    expect(action).toHaveTextContent("Mining Initiative — mine Lodor (31) · cultural");
    expect(within(action).getByLabelText("3 resources")).toBeInTheDocument();
    expect(within(action).getByLabelText("1 influence")).toBeInTheDocument();
    expect(action.querySelector("strong")).toHaveTextContent("Lodor");
    // The source and prompt stay visible in the confirm state.
    expect(screen.getByTestId("planet-selection-source")).toHaveTextContent("Mining Initiative");
    fireEvent.click(screen.getByTestId("confirm-planet-btn"));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith("lodor"));
  });

  it("selects via the chip list and cancels back to selecting", () => {
    render(<Host choice={mining} onSubmit={vi.fn()} />);
    fireEvent.click(screen.getByTestId("planet-chip-gral"));
    expect(screen.getByTestId("planet-chip-gral")).toHaveAttribute("aria-pressed", "true");
    expect(screen.getByTestId("planet-selection-action")).toHaveTextContent(
      "mine Gral (11) · propulsion skip",
    );
    expect(
      within(screen.getByTestId("planet-selection-action")).getByLabelText("1 resource"),
    ).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("cancel-planet-btn"));
    expect(screen.queryByTestId("planet-selection-action")).not.toBeInTheDocument();
  });

  it("offers one button per option on a planet with several options, then confirms", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<Host choice={structures} onSubmit={onSubmit} initialPlanet="lodor" />);
    expect(screen.getByTestId("planet-selection-action")).toHaveTextContent("on Lodor");
    expect(screen.queryByTestId("confirm-planet-btn")).not.toBeInTheDocument();
    expect(screen.getByTestId("planet-option-pds|26|lodor")).toHaveTextContent(
      "Place pds on lodor",
    );
    fireEvent.click(screen.getByTestId("planet-option-spacedock|26|lodor"));
    expect(screen.getByTestId("planet-selection-action")).toHaveTextContent(
      "Place Structure — place spacedock on Lodor",
    );
    fireEvent.click(screen.getByTestId("confirm-planet-btn"));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith("spacedock|26|lodor"));
  });

  it("place_structure says where, the cost and what is already on the planet, keeping the ids", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<Host choice={structures} onSubmit={onSubmit} initialPlanet="lodor" />);
    expect(screen.getByTestId("structure-info-where")).toHaveTextContent("Lodor in Lodor (#26)");
    expect(screen.getByTestId("structure-info-cost")).toHaveTextContent(/No cost/);
    expect(screen.getByTestId("structure-info-there")).toHaveTextContent(
      "Already there: p1 2 infantry, 1 PDS",
    );
    expect(screen.getByTestId("planet-chip-lodor")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("planet-option-pds|26|lodor"));
    fireEvent.click(screen.getByTestId("confirm-planet-btn"));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith("pds|26|lodor"));
  });

  it("Construction numbers its placements and says when only a PDS is allowed", () => {
    const first = { ...structures, details: { step: 1, of: 2 } };
    const { rerender } = render(<Host choice={first} onSubmit={vi.fn()} />);
    expect(screen.getByTestId("structure-step")).toHaveTextContent("Structure 1 of 2");
    expect(screen.getByTestId("structure-step")).not.toHaveTextContent("PDS only");
    rerender(
      <Host choice={{ ...structures, details: { step: 2, of: 2, only_pds: true } }} onSubmit={vi.fn()} />,
    );
    expect(screen.getByTestId("structure-step")).toHaveTextContent("Structure 2 of 2 · PDS only");
  });

  it("shows no step label when the engine sent none", () => {
    render(<Host choice={structures} onSubmit={vi.fn()} />);
    expect(screen.queryByTestId("structure-step")).toBeNull();
  });

  it("submits the decline option directly", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<Host choice={structures} onSubmit={onSubmit} />);
    expect(screen.getByTestId("decline-planet-btn")).toHaveTextContent("Skip");
    fireEvent.click(screen.getByTestId("decline-planet-btn"));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith("decline"));
  });

  it("shows extra non-planet options as chips and confirms them", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<Host choice={bioStims} onSubmit={onSubmit} />);
    expect(screen.getByTestId("planet-selection-source")).toHaveTextContent("Bio-Stims");
    fireEvent.click(screen.getByTestId("extra-option-ready|technology|st"));
    expect(screen.getByTestId("planet-selection-action")).toHaveTextContent(
      "Bio-Stims — Ready Sarween Tools",
    );
    fireEvent.click(screen.getByTestId("confirm-planet-btn"));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith("ready|technology|st"));
  });

  it("uses a specific option label for the planet action", () => {
    render(<Host choice={bioStims} onSubmit={vi.fn()} initialOption="ready|planet|gral" />);
    expect(screen.getByTestId("planet-selection-action")).toHaveTextContent(
      "Bio-Stims — ready Gral (11)",
    );
  });

  it("tells other seats who is choosing a planet and for what", () => {
    render(<PlanetSelectionBar choice={mining} viewerSeat="p2" onSubmit={vi.fn()} />);
    expect(
      screen.getByText(/Waiting for p1 to choose a planet \(Mining Initiative\)/),
    ).toBeInTheDocument();
    expect(screen.queryByTestId("planet-chip-lodor")).not.toBeInTheDocument();
  });

  it("shows submission errors and the server error", async () => {
    const onSubmit = vi.fn().mockRejectedValue(new Error("Planet is no longer offered"));
    const { unmount } = render(<Host choice={mining} onSubmit={onSubmit} initialOption="lodor" />);
    fireEvent.click(screen.getByTestId("confirm-planet-btn"));
    expect(await screen.findByTestId("planet-selection-error")).toHaveTextContent(
      "Planet is no longer offered",
    );
    unmount();
    render(<Host choice={mining} onSubmit={vi.fn()} lastError="Stale decision" />);
    expect(screen.getByRole("alert")).toHaveTextContent("Stale decision");
  });

  it("keeps the picked planet locally when the host does not lift it", () => {
    const onSelectOption = vi.fn();
    render(
      <PlanetSelectionBar
        choice={structures}
        viewerSeat="p1"
        onSelectOption={onSelectOption}
        onSubmit={vi.fn()}
        boardView={board}
      />,
    );
    fireEvent.click(screen.getByTestId("planet-chip-lodor"));
    expect(onSelectOption).toHaveBeenCalledWith("");
    expect(screen.getByTestId("planet-option-pds|26|lodor")).toBeInTheDocument();
  });

  it("names the mech, the planet and the cost for Orbital Drop and keeps confirm and pass", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const drop: PendingChoiceDto = {
      nonce: "od1",
      actor: "p1",
      prompt: "Orbital Drop: deploy a mech on lodor",
      context: { subtype: "orbital_drop_deploy_mech", optional: true },
      options: [
        {
          id: "deploy|sol_mech|1",
          kind: "produce",
          label: "deploy 1 sol_mech for 3 resources",
          payload: { unit: "sol_mech", count: 1, cost: 3, system: "26", planet: "lodor", orbital_drop_deploy: true },
        },
        { id: "decline", kind: "decline", label: "Pass" },
      ],
    };
    render(
      <PlanetSelectionBar choice={drop} viewerSeat="p1" boardView={board} onSubmit={onSubmit} />,
    );
    fireEvent.click(screen.getByTestId("planet-chip-lodor"));
    const bar = screen.getByTestId("planet-selection-bar");
    expect(bar).toHaveTextContent("Deploy Mech");
    expect(bar).toHaveTextContent("Lodor");
    expect(bar).not.toHaveTextContent("sol_mech");
    expect(screen.getByTestId("unit-ability-note")).toHaveTextContent("Cost: 3");
    expect(within(screen.getByTestId("unit-ability-note")).getByLabelText("3 resources")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("confirm-planet-btn"));
    await waitFor(() => expect(onSubmit).toHaveBeenCalledWith("deploy|sol_mech|1"));
  });
});
