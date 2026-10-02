import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
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
    expect(screen.getByText("1 Res / 6 Inf")).toBeInTheDocument();
    expect(screen.getAllByText(/Unknown participant/).length).toBeGreaterThan(0);
    expect(screen.getByTestId("system-inspector").textContent).not.toContain("seat_a");
    expect(screen.getByText("Attachments: custodians")).toBeInTheDocument();

    // Units grouped by player and type, without repeating player info inside unit chips
    expect(screen.getByTestId("inspector-player-space-seat_a")).toHaveTextContent(
      "Unknown participant",
    );
    expect(screen.getByTestId("inspector-space-unit")).toHaveTextContent(
      "1 × Dreadnought (1 damaged)",
    );
    expect(screen.getByTestId("inspector-space-unit").textContent).not.toContain(
      "Unknown participant",
    );

    expect(screen.getByTestId("inspector-player-ground-mecatol_rex-seat_a")).toHaveTextContent(
      "Unknown participant",
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
});
