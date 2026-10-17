import { describe, it, expect, vi } from "vitest";
import { act, render, screen, fireEvent, within } from "@testing-library/react";
import { SystemInspector } from "./SystemInspector.tsx";
import { SelectedSystemDetails } from "../presentation/boardPresentation.ts";

const mockSystem: SelectedSystemDetails = {
  systemId: "18",
  label: "Mecatol Rex",
  anomalies: ["supernova"],
  wormholes: ["alpha"],
  specialArea: null,
  isActiveSystem: true,
  planets: [
    {
      id: "mecatol_rex",
      label: "Mecatol Rex",
      resources: 1,
      influence: 6,
      traits: ["cultural"],
      techSpecialties: [],
      legendary: true,
      controlledBy: "seat_a",
      controllerColor: "#ef4444",
      exhausted: false,
      attachments: ["custodians"],
      isCandidateTarget: true,
      isContextSubject: false,
      associatedOptionIds: ["opt_produce"],
    },
  ],
  spaceUnits: [{ unitType: "dreadnought", owner: "seat_a", ownerColor: "#ef4444", damaged: true }],
  planetUnits: {
    mecatol_rex: [{ unitType: "infantry", owner: "seat_a", ownerColor: "#ef4444", damaged: false }],
  },
  commandTokens: [{ owner: "seat_a", color: "#ef4444" }],
  availableActions: [
    { optionId: "opt_produce", label: "Produce 2 Fighters", kind: "produce_unit" },
  ],
};

describe("SystemInspector Component", () => {
  it("renders system details, anomalies, wormholes, and planets", () => {
    const onClose = vi.fn();
    render(<SystemInspector system={mockSystem} onClose={onClose} />);

    expect(screen.getByTestId("system-inspector")).toBeInTheDocument();
    expect(screen.getByTestId("inspector-system-title")).toHaveTextContent("Mecatol Rex");
    expect(screen.getByText("ACTIVE SYSTEM")).toBeInTheDocument();
    expect(screen.getByTestId("inspector-anomaly")).toHaveTextContent("SUPERNOVA");
    expect(screen.getByTestId("inspector-wormhole")).toHaveTextContent("WORMHOLE: ALPHA");

    // Planet
    expect(screen.getByTestId("inspector-planet-mecatol_rex")).toBeInTheDocument();
    expect(screen.getByLabelText("1 resource")).toBeInTheDocument();
    expect(screen.getByLabelText("6 influence")).toBeInTheDocument();
    expect(screen.getAllByText(/Unknown participant/).length).toBeGreaterThan(0);
    expect(screen.getByTestId("system-inspector").textContent).not.toContain("seat_a");
    expect(screen.getByText("Attachments: Custodians")).toBeInTheDocument();

    // Units grouped by player and type, without repeating player info inside unit chips
    expect(screen.getByTestId("inspector-owner-seat_a")).toHaveTextContent("Unknown participant");
    expect(screen.getByTestId("inspector-player-space-seat_a")).toBeInTheDocument();
    expect(screen.getByTestId("inspector-space-unit")).toHaveTextContent(
      "1 × Dreadnought (1 damaged)",
    );
    expect(screen.getByTestId("inspector-space-unit").textContent).not.toContain(
      "Unknown participant",
    );

    expect(screen.getByTestId("inspector-player-ground-mecatol_rex-seat_a")).toHaveTextContent(
      "On Mecatol Rex",
    );
    expect(screen.getByTestId("inspector-ground-unit")).toHaveTextContent("1 × Infantry");
    expect(screen.getByTestId("inspector-ground-unit").textContent).not.toContain(
      "Unknown participant",
    );

    // Command Token
    expect(screen.getByTestId("inspector-command-token")).toHaveTextContent("Unknown participant");

    // Close button
    fireEvent.click(screen.getByTestId("close-inspector-button"));
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("groups multiple units of the same and different types with damage counts", () => {
    const multiUnitSystem: SelectedSystemDetails = {
      ...mockSystem,
      spaceUnits: [
        { unitType: "carrier", owner: "seat_a", ownerColor: "#E69F00", damaged: false },
        { unitType: "carrier", owner: "seat_a", ownerColor: "#E69F00", damaged: false },
        { unitType: "destroyer", owner: "seat_a", ownerColor: "#E69F00", damaged: false },
        { unitType: "fighter", owner: "seat_a", ownerColor: "#E69F00", damaged: false },
        { unitType: "fighter", owner: "seat_a", ownerColor: "#E69F00", damaged: false },
        { unitType: "fighter", owner: "seat_a", ownerColor: "#E69F00", damaged: false },
        { unitType: "dreadnought", owner: "seat_a", ownerColor: "#E69F00", damaged: true },
        { unitType: "dreadnought", owner: "seat_a", ownerColor: "#E69F00", damaged: false },
      ],
      planetUnits: {
        mecatol_rex: [
          { unitType: "infantry", owner: "seat_a", ownerColor: "#E69F00", damaged: false },
          { unitType: "infantry", owner: "seat_a", ownerColor: "#E69F00", damaged: false },
          { unitType: "mech", owner: "seat_a", ownerColor: "#E69F00", damaged: true },
        ],
      },
    };

    render(<SystemInspector system={multiUnitSystem} onClose={() => {}} />);

    const spaceUnits = screen.getAllByTestId("inspector-space-unit");
    expect(spaceUnits).toHaveLength(4);
    // Priority order: dreadnought, carrier, destroyer, fighter
    expect(spaceUnits[0]).toHaveTextContent("2 × Dreadnoughts (1 damaged)");
    expect(spaceUnits[1]).toHaveTextContent("2 × Carriers");
    expect(spaceUnits[2]).toHaveTextContent("1 × Destroyer");
    expect(spaceUnits[3]).toHaveTextContent("3 × Fighters");

    const groundUnits = screen.getAllByTestId("inspector-ground-unit");
    expect(groundUnits).toHaveLength(2);
    // Priority order: mech, infantry
    expect(groundUnits[0]).toHaveTextContent("1 × Mech (1 damaged)");
    expect(groundUnits[1]).toHaveTextContent("2 × Infantry");
  });

  it("does not render available decisions or action buttons in the system inspector", () => {
    render(<SystemInspector system={mockSystem} onClose={() => {}} />);

    expect(screen.queryByText(/Available Decision Actions/i)).not.toBeInTheDocument();
    expect(screen.queryByTestId("inspector-action-opt_produce")).not.toBeInTheDocument();
  });

  it("renders nothing when system is null", () => {
    const { container } = render(<SystemInspector system={null} onClose={() => {}} />);
    expect(container.firstChild).toBeNull();
  });

  it("dismisses on Escape key", () => {
    const onClose = vi.fn();
    render(<SystemInspector system={mockSystem} onClose={onClose} />);

    fireEvent.keyDown(window, { key: "Escape" });
    expect(onClose).toHaveBeenCalledTimes(1);
  });

  it("explains what the anomaly does and shows a legendary planet's ability", () => {
    const system = {
      ...mockSystem,
      anomalies: ["gravity rift"],
      planets: [{ ...mockSystem.planets[0], id: "primor", label: "Primor", legendary: true }],
      planetUnits: {},
    };
    render(<SystemInspector system={system} onClose={vi.fn()} />);
    expect(screen.getByTestId("inspector-anomaly-rule")).toHaveTextContent(
      /Gravity rift:.*\+1 to its move value.*1 to 3/,
    );
    expect(screen.getByTestId("inspector-legendary-primor")).toHaveTextContent(/The Atrament/i);
  });

  it("shows a rival's unit card on hover", async () => {
    vi.useFakeTimers();
    render(<SystemInspector system={mockSystem} onClose={vi.fn()} />);
    const chip = screen.getByTestId("inspector-space-unit").querySelector("[data-unit-stats]")!;
    fireEvent.pointerEnter(chip);
    await act(async () => {
      vi.advanceTimersByTime(300);
    });
    vi.useRealTimers();
    expect(screen.getByTestId("unit-info-card")).toHaveTextContent(/Dreadnought/);
  });

  it("lists units strictly per owner: the viewer first, then the table order, never mixed", () => {
    const mixed: SelectedSystemDetails = {
      ...mockSystem,
      spaceUnits: [
        { unitType: "cruiser", owner: "seat_c", ownerColor: "#0f0", damaged: false },
        { unitType: "carrier", owner: "seat_b", ownerColor: "#00f", damaged: false },
        { unitType: "dreadnought", owner: "seat_a", ownerColor: "#f00", damaged: false },
        { unitType: "cruiser", owner: "seat_b", ownerColor: "#00f", damaged: false },
      ],
      planetUnits: {
        mecatol_rex: [
          { unitType: "infantry", owner: "seat_c", ownerColor: "#0f0", damaged: false, planet: "mecatol_rex" },
          { unitType: "pds", owner: "seat_a", ownerColor: "#f00", damaged: false, planet: "mecatol_rex" },
          { unitType: "infantry", owner: "seat_a", ownerColor: "#f00", damaged: false, planet: "mecatol_rex" },
        ],
      },
    };
    render(
      <SystemInspector
        system={mixed}
        onClose={() => {}}
        viewerSeat="seat_b"
        seatingOrder={["seat_a", "seat_b", "seat_c"]}
      />,
    );
    const blocks = screen.getAllByTestId(/^inspector-owner-seat_[abc]$/);
    expect(blocks.map((b) => b.dataset.testid)).toEqual([
      "inspector-owner-seat_b",
      "inspector-owner-seat_a",
      "inspector-owner-seat_c",
    ]);
    const own = within(blocks[0]);
    expect(own.getAllByTestId("inspector-space-unit").map((e) => e.textContent)).toEqual([
      expect.stringContaining("Carrier"),
      expect.stringContaining("Cruiser"),
    ]);
    expect(own.queryByTestId("inspector-ground-unit")).toBeNull();
    const a = within(blocks[1]);
    expect(a.getByTestId("inspector-space-unit")).toHaveTextContent("Dreadnought");
    expect(a.getByTestId("inspector-ground-unit")).toHaveTextContent("Infantry");
    expect(a.getByTestId("inspector-owner-structures-mecatol_rex-seat_a")).toHaveTextContent("PDS");
    const c = within(blocks[2]);
    expect(c.getByTestId("inspector-space-unit")).toHaveTextContent("Cruiser");
    expect(c.getByTestId("inspector-ground-unit")).toHaveTextContent("Infantry");
  });

  it("marks a planet raised by an attachment and names the source", () => {
    const boosted: SelectedSystemDetails = {
      ...mockSystem,
      planets: [
        {
          ...mockSystem.planets[0],
          resources: 3,
          influence: 6,
          boosts: [{ source: "Mining World", resources: 2, influence: 0 }],
        },
      ],
    };
    render(<SystemInspector system={boosted} onClose={vi.fn()} />);
    const resources = screen.getByLabelText(/^3 resources/);
    expect(resources).toHaveAttribute("data-boosted", "true");
    expect(resources).toHaveAttribute("title", "3 resources (1 printed; Mining World: +2 resources)");
    expect(screen.getByLabelText("6 influence")).not.toHaveAttribute("data-boosted");
  });
});
