import { describe, it, expect } from "vitest";
import { render, screen, within } from "@testing-library/react";
import { PlanetStructures } from "./PlanetStructures.tsx";
import { SystemInspector } from "./SystemInspector.tsx";
import { StructureIcon, STRUCTURE_GLYPHS } from "./StructureIcon.tsx";
import { SeatInfoProvider } from "../presentation/SeatInfoContext.tsx";
import { splitStructures, groupStructures } from "../presentation/planetStructures.ts";
import type { PlacedUnitPresentation, SelectedSystemDetails } from "../presentation/boardPresentation.ts";
import type { PlayerView } from "../protocol/types.ts";

const unit = (unitType: string, owner = "seat_a", damaged = false): PlacedUnitPresentation => ({
  unitType,
  owner,
  ownerColor: owner === "seat_a" ? "#E69F00" : "#56B4E9",
  planet: "jord",
  damaged,
});

const players = [
  { id: "seat_a", faction: "sol", technologies: ["pds2"] },
  { id: "seat_b", faction: "hacan", technologies: [] },
] as unknown as PlayerView[];

const renderCard = (units: PlacedUnitPresentation[], seats = players) =>
  render(
    <SeatInfoProvider players={seats}>
      <PlanetStructures planetId="jord" units={units} />
    </SeatInfoProvider>,
  );

describe("PlanetStructures", () => {
  it("shows a lone PDS with its space cannon", () => {
    renderCard([unit("pds", "seat_b")]);
    const row = screen.getByTestId("planet-structure-jord-pds");
    expect(row).toHaveTextContent("PDS");
    expect(row).toHaveTextContent(/Space cannon 6/);
    expect(screen.queryByTestId("planet-structure-jord-spacedock")).toBeNull();
  });

  it("shows a lone space dock with its production", () => {
    renderCard([unit("spacedock")]);
    expect(screen.getByTestId("planet-structure-jord-spacedock")).toHaveTextContent(/Production/);
    expect(screen.queryByTestId("planet-structure-jord-pds")).toBeNull();
  });

  it("shows both, PDS first", () => {
    renderCard([unit("spacedock"), unit("pds")]);
    const rows = within(screen.getByTestId("planet-structures-jord")).getAllByTestId(/^planet-structure-/);
    expect(rows.map((r) => r.getAttribute("data-testid"))).toEqual([
      "planet-structure-jord-pds",
      "planet-structure-jord-spacedock",
    ]);
  });

  it("uses the upgraded card of an owner who researched it", () => {
    renderCard([unit("pds", "seat_a"), unit("pds", "seat_b")]);
    const [mine, theirs] = screen.getAllByTestId("planet-structure-jord-pds");
    expect(mine).toHaveTextContent("PDS II");
    expect(theirs).not.toHaveTextContent("PDS II");
    expect(mine).toHaveTextContent(/Space cannon 5/);
  });

  it("marks another owner's structure with that owner's colour", () => {
    renderCard([unit("pds", "seat_b")]);
    const row = screen.getByTestId("planet-structure-jord-pds");
    expect(row).toHaveAttribute("data-owner", "seat_b");
    expect(row.querySelector("svg")).toHaveStyle({ color: "#56B4E9" });
  });

  it("counts several and flags damage", () => {
    renderCard([unit("pds", "seat_b", true), unit("pds", "seat_b")]);
    const row = screen.getByTestId("planet-structure-jord-pds");
    expect(row).toHaveTextContent("2 × PDS");
    expect(row).toHaveTextContent("1 damaged");
  });

  it("names the icon for assistive tech without raw ids", () => {
    renderCard([unit("spacedock")]);
    const icon = screen.getByRole("img");
    expect(icon.getAttribute("aria-label")).toMatch(/^Space Dock/);
    expect(icon.getAttribute("aria-label")).not.toMatch(/spacedock|seat_a/);
  });

  it("renders nothing without structures", () => {
    const { container } = renderCard([unit("infantry")]);
    expect(container).toBeEmptyDOMElement();
  });
});

describe("structure helpers", () => {
  it("separates structures from ground forces and ignores space-area docks", () => {
    const inSpace = { ...unit("spacedock"), planet: null };
    const { structures, others } = splitStructures([unit("pds"), unit("infantry"), inSpace]);
    expect(structures.map((u) => u.unitType)).toEqual(["pds"]);
    expect(others).toHaveLength(2);
    expect(groupStructures(structures)).toHaveLength(1);
  });

  it("recognises faction variants", () => {
    expect(splitStructures([unit("titans_pds"), unit("saar_spacedock2")]).structures).toHaveLength(2);
  });
});

describe("StructureIcon", () => {
  it.each(["pds", "spacedock"] as const)("draws %s in the owner colour with a name", (kind) => {
    const { container } = render(<StructureIcon kind={kind} size={28} color="#CC79A7" label="Name, Sol" />);
    const svg = container.querySelector("svg")!;
    expect(svg).toHaveAttribute("width", "28");
    expect(svg).toHaveStyle({ color: "#CC79A7" });
    expect(svg).toHaveAttribute("aria-label", "Name, Sol");
    expect(svg.querySelector("title")).toHaveTextContent("Name, Sol");
  });

  it("is decorative without a label and maps both kinds", () => {
    const { container } = render(<StructureIcon kind="pds" />);
    expect(container.querySelector("svg")).toHaveAttribute("aria-hidden", "true");
    expect(Object.keys(STRUCTURE_GLYPHS).sort()).toEqual(["pds", "spacedock"]);
  });
});

describe("SystemInspector planet card", () => {
  const system: SelectedSystemDetails = {
    systemId: "18",
    label: "Mecatol Rex",
    anomalies: [],
    wormholes: [],
    isActiveSystem: false,
    planets: ["jord", "quann"].map((id) => ({
      id,
      label: id,
      resources: 2,
      influence: 1,
      traits: [],
      techSpecialties: [],
      legendary: false,
      controlledBy: "seat_a",
      controllerColor: "#E69F00",
      exhausted: false,
      attachments: [],
      isCandidateTarget: false,
      isContextSubject: false,
      associatedOptionIds: [],
    })),
    spaceUnits: [],
    planetUnits: {
      jord: [unit("pds"), unit("spacedock"), unit("infantry")],
      quann: [{ ...unit("pds"), planet: "quann" }],
    },
    commandTokens: [],
    availableActions: [],
  };

  it("puts structures in each planet's card and not in the unit list", () => {
    render(<SystemInspector system={system} onClose={() => {}} />);
    const jord = screen.getByTestId("inspector-planet-jord");
    expect(within(jord).getByTestId("planet-structure-jord-pds")).toBeInTheDocument();
    expect(within(jord).getByTestId("planet-structure-jord-spacedock")).toBeInTheDocument();
    expect(within(screen.getByTestId("inspector-planet-quann")).getByTestId("planet-structure-quann-pds")).toBeInTheDocument();
    const groundChips = screen.getAllByTestId("inspector-ground-unit");
    expect(groundChips).toHaveLength(1);
    expect(groundChips[0]).toHaveTextContent("Infantry");
    expect(screen.getByText(/^Units/)).toHaveTextContent("Units (1)");
  });
});
