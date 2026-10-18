import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, within } from "@testing-library/react";
import { Board, getPlayerColor } from "./Board.tsx";
import { BoardView } from "../protocol/types.ts";
import { PaymentDraftProvider } from "../presentation/PaymentDraftContext.tsx";
import {
  ProductionPaymentProvider,
  type ProductionPaymentApi,
} from "../presentation/ProductionPaymentContext.tsx";
import { partialContext } from "../test/wire.ts";

const mockBoard: BoardView = {
  systems: {
    "18": {
      system_id: "18",
      planets: {
        mecatol_rex: {
          planet_id: "mecatol_rex",
          controlled_by: null,
          exhausted: false,
          attachments: [],
        },
      },
      units: [
        { unit_type: "cruiser", owner: "p1", damaged: false, planet: null, galvanized: false },
        { unit_type: "infantry", owner: "p1", damaged: false, planet: null, galvanized: false },
      ],
      command_tokens: ["p1"],
    },
    "34": {
      system_id: "34",
      planets: {
        abyz: {
          planet_id: "abyz",
          controlled_by: "p2",
          exhausted: true,
          attachments: [],
        },
        fria: {
          planet_id: "fria",
          controlled_by: "p2",
          exhausted: false,
          attachments: [],
        },
      },
      units: [],
      command_tokens: [],
    },
  },
};

describe("Board Component", () => {
  it("renders all systems in SVG with correct coordinates and planet labels", () => {
    render(<Board board={mockBoard} seatingOrder={["p1", "p2"]} />);

    expect(screen.getByTestId("ti4-board-svg")).toBeInTheDocument();
    const map = screen.getByRole("group", { name: "Galaxy map" });
    expect(map).toBe(screen.getByTestId("ti4-board-svg"));
    expect(map).toHaveAccessibleDescription(/Drag the map to pan/);
    expect(screen.getByTestId("system-hex-18")).toBeInTheDocument();
    expect(screen.getByTestId("system-hex-34")).toBeInTheDocument();

    // Mecatol Rex text
    expect(screen.getByText("Mecatol Rex")).toBeInTheDocument();
    expect(screen.getByText("#34")).toBeInTheDocument();

    // Planet abbreviations
    expect(screen.getByText("MEC")).toBeInTheDocument();
    expect(screen.getByText("ABY")).toBeInTheDocument();
    expect(screen.getByText("FRI")).toBeInTheDocument();

    // Units label
    expect(screen.getByText("2 units")).toBeInTheDocument();
  });

  it("makes every system keyboard-inspectable", () => {
    render(<Board board={mockBoard} seatingOrder={["p1", "p2"]} />);

    const hex18 = screen.getByTestId("system-hex-18");
    const hex34 = screen.getByTestId("system-hex-34");
    expect(hex18).toHaveAttribute("role", "button");
    expect(hex18).toHaveAttribute("tabindex", "0");
    expect(hex34).toHaveAttribute("aria-label", "Inspect system #34 #34");
  });

  it("assigns colors by projected seating order, not seat name", () => {
    expect(getPlayerColor("unusual-seat", ["unusual-seat", "another-seat"])).toBe("#3B82F6");
    expect(getPlayerColor("another-seat", ["unusual-seat", "another-seat"])).toBe("#4ADE80");
    expect(getPlayerColor("absent-seat", ["unusual-seat"])).toBe("#94a3b8");
  });

  it("renders static map_tiles with anomalies, wormholes, and zoom controls", () => {
    const boardWithMap: BoardView = {
      systems: {
        "18": {
          system_id: "18",
          command_tokens: [],
          planets: {},
          units: [{ unit_type: "carrier", owner: "p1", damaged: false, planet: null, galvanized: false }],
        },
      },
      map_tiles: [
        {
          system_id: "18",
          label: "Mecatol Rex",
          q: 0,
          r: 0,
          planets: [{ id: "mecatol_rex", label: "Mecatol Rex", resources: 1, influence: 6 }],
        },
        {
          system_id: "67",
          label: "Cormund",
          q: 1,
          r: -1,
          anomalies: ["gravity rift"],
          wormholes: ["alpha"],
          planets: [],
        },
      ],
    };

    render(<Board board={boardWithMap} seatingOrder={["p1"]} />);

    expect(screen.getByTestId("system-hex-18")).toBeInTheDocument();
    expect(screen.getByTestId("system-hex-67")).toBeInTheDocument();
    expect(screen.getByText("Mecatol Rex")).toBeInTheDocument();
    expect(screen.getByText("GRAVITY RIFT")).toBeInTheDocument();
    expect(screen.getByText("α")).toBeInTheDocument();
    expect(screen.getByLabelText("1 resource, 6 influence")).toBeInTheDocument();

    // Zoom buttons
    expect(screen.getByTitle("Zoom In")).toBeInTheDocument();
    expect(screen.getByTitle("Zoom Out")).toBeInTheDocument();
    expect(screen.getByTitle("Reset Pan & Zoom")).toBeInTheDocument();
  });

  it("highlights candidate targets on the board for the deciding actor and handles clicks", () => {
    const onSelectTarget = vi.fn();
    const onSelectSystem = vi.fn();
    const pendingChoice = {
      nonce: "nonce_test",
      actor: "p1",
      prompt: "Activate a system",
      options: [
        {
          id: "opt_activate_18",
          kind: "activate",
          label: "Activate Mecatol Rex",
          payload: { system: "18" },
        },
      ],
    };

    render(
      <Board
        board={mockBoard}
        seatingOrder={["p1", "p2"]}
        pendingChoice={pendingChoice}
        viewerSeat="p1"
        onSelectTarget={onSelectTarget}
        onSelectSystem={onSelectSystem}
      />,
    );

    const hex18 = screen.getByTestId("system-hex-18");
    const hex34 = screen.getByTestId("system-hex-34");

    expect(hex18).toHaveAttribute("data-target-candidate", "true");
    expect(hex18).toHaveAttribute("role", "button");
    expect(hex18).toHaveAttribute("tabindex", "0");

    expect(hex34).not.toHaveAttribute("data-target-candidate");

    // Click candidate target hex
    fireEvent.click(hex18);
    expect(onSelectTarget).toHaveBeenCalledWith("18");
    expect(onSelectSystem).toHaveBeenCalledWith("18");
  });

  it("redacts target candidate highlighting when viewer is not the actor", () => {
    const pendingChoice = {
      nonce: "nonce_test",
      actor: "p1",
      prompt: "Activate a system",
      options: [
        {
          id: "opt_activate_18",
          kind: "activate",
          label: "Activate Mecatol Rex",
          payload: { system: "18" },
        },
      ],
    };

    render(
      <Board
        board={mockBoard}
        seatingOrder={["p1", "p2"]}
        pendingChoice={pendingChoice}
        viewerSeat="p2"
      />,
    );

    const hex18 = screen.getByTestId("system-hex-18");
    expect(hex18).not.toHaveAttribute("data-target-candidate");
    expect(hex18).toHaveAttribute("aria-label", "Inspect system Mecatol Rex #18");
    expect(screen.queryByTestId("activation-target-reticle")).not.toBeInTheDocument();
  });

  it("displays SystemInspector when a system is selected", () => {
    render(<Board board={mockBoard} seatingOrder={["p1", "p2"]} selectedSystemId="18" />);

    expect(screen.getByTestId("system-inspector")).toBeInTheDocument();
    expect(screen.getByTestId("inspector-system-title")).toHaveTextContent("Mecatol Rex");
  });

  it("pins inspection on click and closes via its close button or empty map", () => {
    const onSelectSystem = vi.fn();
    render(<Board board={mockBoard} seatingOrder={["p1", "p2"]} onSelectSystem={onSelectSystem} />);
    fireEvent.click(screen.getByTestId("system-hex-18"));
    expect(screen.getByTestId("system-inspector")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("system-hex-34"));
    expect(screen.getByTestId("inspector-system-title")).toHaveTextContent("#34");
    fireEvent.click(screen.getByTestId("close-inspector-button"));
    expect(screen.queryByTestId("system-inspector")).not.toBeInTheDocument();
    fireEvent.keyDown(screen.getByTestId("system-hex-18"), { key: "Enter" });
    expect(screen.getByTestId("system-inspector")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("ti4-board-svg"));
    expect(screen.queryByTestId("system-inspector")).not.toBeInTheDocument();
    expect(onSelectSystem).toHaveBeenCalledWith(null);
  });

  it("assigns accessible button semantics and keyboard activation to candidate target planets", () => {
    const onSelectTarget = vi.fn();
    const pendingChoice = {
      nonce: "nonce_planet",
      actor: "p1",
      prompt: "Commit ground forces to planet",
      options: [
        {
          id: "opt_land_mecatol",
          kind: "commit_ground_forces",
          label: "Land on Mecatol Rex",
          payload: { system: "18", planet: "mecatol_rex" },
        },
      ],
    };

    render(
      <Board
        board={mockBoard}
        seatingOrder={["p1", "p2"]}
        pendingChoice={pendingChoice}
        viewerSeat="p1"
        onSelectTarget={onSelectTarget}
      />,
    );

    const planetMecatol = screen.getByTestId("planet-mecatol_rex");
    const planetAbyz = screen.getByTestId("planet-abyz");

    expect(planetMecatol).toHaveAttribute("data-target-candidate", "true");
    expect(planetMecatol).toHaveAttribute("role", "button");
    expect(planetMecatol).toHaveAttribute("tabindex", "0");
    expect(planetMecatol).toHaveAttribute("aria-label", "Target planet mecatol_rex");

    expect(planetAbyz).not.toHaveAttribute("data-target-candidate");
    expect(planetAbyz).not.toHaveAttribute("role");

    // Keyboard activation via Enter
    fireEvent.keyDown(planetMecatol, { key: "Enter" });
    expect(onSelectTarget).toHaveBeenCalledWith("18", "mecatol_rex");

    // Keyboard activation via Space
    fireEvent.keyDown(planetMecatol, { key: " " });
    expect(onSelectTarget).toHaveBeenCalledWith("18", "mecatol_rex");
    expect(onSelectTarget).toHaveBeenCalledTimes(2);
  });

  it("closes the inspector of a system tapped to answer a pick once that decision is answered", () => {
    // Offline runs o2-04-2037, o2-11-2050, o2-21-2111 (phone): the system activated rounds
    // earlier was still selected, and its inspector covered the map at the next activation.
    const activation = {
      nonce: "nonce_act",
      actor: "p1",
      prompt: "activate a system",
      context: partialContext({ subtype: "activate_system" }),
      options: [{ id: "18", label: "Mecatol Rex", kind: "activate", payload: { system: "18" } }],
    };
    const next = {
      nonce: "nonce_move",
      actor: "p1",
      prompt: "movement",
      options: [{ id: "done_moving", kind: "decline", label: "finish movement", payload: {} }],
    };
    const { rerender } = render(
      <Board board={mockBoard} seatingOrder={["p1", "p2"]} pendingChoice={activation} viewerSeat="p1" />,
    );
    fireEvent.click(screen.getByTestId("system-hex-18"));
    expect(screen.getByTestId("inspector-system-title")).toHaveTextContent("#18");
    rerender(<Board board={mockBoard} seatingOrder={["p1", "p2"]} pendingChoice={next} viewerSeat="p1" />);
    expect(screen.queryByTestId("system-inspector")).not.toBeInTheDocument();

    // A system the player opened to look at stays open.
    fireEvent.click(screen.getByTestId("system-hex-34"));
    rerender(<Board board={mockBoard} seatingOrder={["p1", "p2"]} pendingChoice={activation} viewerSeat="p1" />);
    expect(screen.getByTestId("inspector-system-title")).toHaveTextContent("#34");
  });

  it("closes an inspector opened to look at a system when an activation becomes pending on a phone", () => {
    // Nightly 2026-10-16 F4 (o2-124, o3-147, o1-167, o2-270, o1-325, o3-340): the inspector stayed
    // open over the map when activate_system came up.
    const activation = {
      nonce: "nonce_act",
      actor: "p1",
      prompt: "activate a system",
      context: partialContext({ subtype: "activate_system" }),
      options: [{ id: "18", label: "Mecatol Rex", kind: "activate", payload: { system: "18" } }],
    };
    const other = {
      nonce: "nonce_other",
      actor: "p1",
      prompt: "something else",
      options: [{ id: "done", kind: "decline", label: "done", payload: {} }],
    };
    const original = window.matchMedia;
    window.matchMedia = ((query: string) => ({
      matches: query.includes("max-width: 720px"),
      media: query,
      addEventListener: () => undefined,
      removeEventListener: () => undefined,
    })) as unknown as typeof window.matchMedia;
    try {
      const { rerender } = render(
        <Board board={mockBoard} seatingOrder={["p1", "p2"]} pendingChoice={other} viewerSeat="p1" />,
      );
      fireEvent.click(screen.getByTestId("system-hex-34"));
      expect(screen.getByTestId("system-inspector")).toBeInTheDocument();
      rerender(<Board board={mockBoard} seatingOrder={["p1", "p2"]} pendingChoice={activation} viewerSeat="p1" />);
      expect(screen.queryByTestId("system-inspector")).not.toBeInTheDocument();
    } finally {
      window.matchMedia = original;
    }
  });

  it("renders activation target reticles during system activation mode", () => {
    const activationChoice = {
      nonce: "nonce_act",
      actor: "p1",
      prompt: "activate a system",
      context: partialContext({ subtype: "activate_system" }),
      options: [{ id: "18", label: "Mecatol Rex", kind: "activate", payload: { system: "18" } }],
    };

    render(
      <Board
        board={mockBoard}
        seatingOrder={["p1", "p2"]}
        pendingChoice={activationChoice}
        viewerSeat="p1"
      />,
    );

    expect(screen.getByTestId("activation-target-reticle")).toBeInTheDocument();
  });

  it("does not render available decision actions inside the system inspector", () => {
    render(
      <Board
        board={mockBoard}
        seatingOrder={["p1", "p2"]}
        selectedSystemId="18"
        viewerSeat="p1"
        pendingChoice={{
          nonce: "n",
          actor: "p1",
          prompt: "choose",
          options: [
            { id: "first", label: "First", payload: { system: "18" }, kind: "" },
            { id: "second", label: "Second", payload: { system: "18" }, kind: "" },
          ],
        }}
      />,
    );
    expect(screen.getByTestId("system-inspector")).toBeInTheDocument();
    expect(screen.queryByText(/Available Decision Actions/i)).not.toBeInTheDocument();
    expect(screen.queryByTestId("inspector-action-second")).not.toBeInTheDocument();
  });

  it("renders animated movement vector lines and badges between origin and destination", () => {
    const movementChoice = {
      nonce: "nonce_move",
      actor: "p1",
      prompt: "movement",
      context: partialContext({
        subtype: "movement_step",
        target: { System: "18" },
      }),
      options: [
        {
          id: "move|34|0",
          label: "Cruiser",
          kind: "move",
          payload: { origin: "34", unit: "cruiser" },
        },
        {
          id: "move|34|1",
          label: "Fighter",
          kind: "move",
          payload: { origin: "34", unit: "fighter" },
        },
        { id: "done_moving", label: "Finish", kind: "decline", payload: {} },
      ],
    };

    render(
      <Board
        board={mockBoard}
        seatingOrder={["p1", "p2"]}
        pendingChoice={movementChoice}
        viewerSeat="p1"
      />,
    );

    const vectorLine = screen.getByTestId("movement-vector-line");
    expect(vectorLine).toBeInTheDocument();
    expect(vectorLine).toHaveAttribute("marker-end", "url(#vector-arrow)");
    expect(vectorLine.parentElement?.querySelector("text")).toHaveTextContent("2"); // 2 units available to move
  });

  describe("Map Overlays", () => {
    const overlayBoard: BoardView = {
      map_tiles: [
        {
          system_id: "201",
          label: "Alpha System",
          q: 0,
          r: 0,
          planets: [
            {
              id: "planet_alpha",
              label: "Alpha Planet",
              resources: 3,
              influence: 1,
              tech_specialties: ["cybernetic"],
            },
          ],
        },
        {
          system_id: "202",
          label: "Beta System",
          q: 1,
          r: 0,
          planets: [
            {
              id: "planet_beta",
              label: "Beta Planet",
              resources: 1,
              influence: 3,
            },
          ],
        },
      ],
      systems: {
        "201": {
          system_id: "201",
          planets: {
            planet_alpha: {
              planet_id: "planet_alpha",
              controlled_by: "p1",
              exhausted: false,
            },
          },
          units: [
            { unit_type: "dreadnought", owner: "p1", damaged: false, planet: null, galvanized: false },
            { unit_type: "cruiser", owner: "p1", damaged: false, planet: null, galvanized: false },
            { unit_type: "infantry", owner: "p1", planet: "planet_alpha", damaged: false, galvanized: false },
            { unit_type: "mech", owner: "p1", planet: "planet_alpha", damaged: false, galvanized: false },
            { unit_type: "pds", owner: "p1", planet: "planet_alpha", damaged: false, galvanized: false },
          ],
          command_tokens: [],
        },
        "202": {
          system_id: "202",
          planets: {
            planet_beta: {
              planet_id: "planet_beta",
              controlled_by: "p2",
              exhausted: true,
            },
          },
          units: [{ unit_type: "destroyer", owner: "p2", damaged: false, planet: null, galvanized: false }],
          command_tokens: [],
        },
      },
    };

    it("toggles the economy overlay and displays ready vs exhausted resources/influence with planets hidden", () => {
      render(<Board board={overlayBoard} seatingOrder={["p1", "p2"]} />);

      expect(screen.queryByTestId("economy-overlay-201")).not.toBeInTheDocument();
      expect(screen.getByTestId("planet-planet_alpha")).toBeInTheDocument();

      // Click Economy overlay button
      fireEvent.click(screen.getByTestId("overlay-btn-economy"));

      // Planets are dropped in economy view to maximize hex focus
      expect(screen.queryByTestId("planet-planet_alpha")).not.toBeInTheDocument();

      // System 201 has ready 3 resources and 1 influence
      const eco201 = screen.getByTestId("economy-overlay-201");
      expect(eco201).toBeInTheDocument();
      expect(eco201).toHaveTextContent("3");
      expect(eco201).toHaveTextContent("1");

      // System 202 has exhausted planet (1 resource / 3 influence printed, 0 ready)
      const eco202 = screen.getByTestId("economy-overlay-202");
      expect(eco202).toBeInTheDocument();
      expect(eco202).toHaveTextContent("0/1");
      expect(eco202).toHaveTextContent("0/3");
      expect(within(eco202).getByLabelText("0 resources ready of 1 total")).toBeInTheDocument();
      expect(within(eco202).getByLabelText("0 influence ready of 3 total")).toBeInTheDocument();
    });

    it("displays space combat units, average hits per round, and sustain damage with planets hidden", () => {
      render(<Board board={overlayBoard} seatingOrder={["p1", "p2"]} overlayMode="space_combat" />);

      // Planets are dropped in space combat view
      expect(screen.queryByTestId("planet-planet_alpha")).not.toBeInTheDocument();

      // System 201 has 1 dreadnought + 1 cruiser in space -> 2 ships, 0.6 + 0.4 = 1.0 hits, 1 sustain
      expect(screen.getByTestId("space-combat-overlay-201")).toBeInTheDocument();
      const fleetP1 = screen.getByTestId("space-combat-fleet-201-p1");
      expect(fleetP1).toHaveTextContent("🚀2");
      expect(fleetP1).toHaveTextContent("1.0");
      expect(fleetP1).toHaveTextContent("🛡️1");

      // System 202 has 1 destroyer -> 1 ship, 0.2 hits, 0 sustain
      expect(screen.getByTestId("space-combat-overlay-202")).toBeInTheDocument();
      const fleetP2 = screen.getByTestId("space-combat-fleet-202-p2");
      expect(fleetP2).toHaveTextContent("🚀1");
      expect(fleetP2).toHaveTextContent("0.2");
      expect(fleetP2).not.toHaveTextContent("🛡️");
    });

    it("marks the hex with a big fight symbol when two players coexist in space combat", () => {
      const contestedBoard: BoardView = {
        map_tiles: [
          {
            system_id: "401",
            label: "Battle Zone",
            q: 0,
            r: 0,
            planets: [],
          },
        ],
        systems: {
          "401": {
            system_id: "401",
            planets: {},
            units: [
              { unit_type: "cruiser", owner: "p1", damaged: false, planet: null, galvanized: false },
              { unit_type: "cruiser", owner: "p2", damaged: false, planet: null, galvanized: false },
            ],
            command_tokens: [],
          },
        },
      };

      render(
        <Board board={contestedBoard} seatingOrder={["p1", "p2"]} overlayMode="space_combat" />,
      );

      expect(screen.getByTestId("space-combat-battle-401")).toBeInTheDocument();
      expect(screen.getByTestId("space-combat-battle-401")).toHaveTextContent("⚔️");
      expect(screen.getByTestId("space-combat-battle-401")).toHaveTextContent("BATTLE");
    });

    it("displays ground combat forces, average hits, and planetary shield on planets", () => {
      render(
        <Board board={overlayBoard} seatingOrder={["p1", "p2"]} overlayMode="ground_combat" />,
      );

      // Planets are preserved in ground combat view
      expect(screen.getByTestId("planet-planet_alpha")).toBeInTheDocument();

      // planet_alpha has 1 infantry (0.3) + 1 mech (0.5) = 2 defenders, 0.8 hits, and PDS planetary shield
      const groundAlpha = screen.getByTestId("ground-combat-overlay-planet_alpha");
      expect(groundAlpha).toBeInTheDocument();
      expect(groundAlpha).toHaveTextContent("2");
      expect(groundAlpha).toHaveTextContent("0.8");
      expect(groundAlpha).toHaveTextContent("🛡️");

      // planet_beta has 0 ground forces
      const groundBeta = screen.getByTestId("ground-combat-overlay-planet_beta");
      expect(groundBeta).toBeInTheDocument();
      expect(groundBeta).toHaveTextContent("0");
    });

    it("staggers multi-planet systems in ground combat so planets do not overlap", () => {
      const multiPlanetBoard: BoardView = {
        map_tiles: [
          {
            system_id: "301",
            label: "Dual System",
            q: 0,
            r: 0,
            planets: [
              { id: "p_left", label: "Left Planet", resources: 2, influence: 0 },
              { id: "p_right", label: "Right Planet", resources: 0, influence: 2 },
            ],
          },
        ],
        systems: {
          "301": {
            system_id: "301",
            planets: {
              p_left: { planet_id: "p_left", controlled_by: "p1", exhausted: false },
              p_right: { planet_id: "p_right", controlled_by: "p2", exhausted: false },
            },
            units: [
              { unit_type: "infantry", owner: "p1", planet: "p_left", damaged: false, galvanized: false },
              { unit_type: "infantry", owner: "p2", planet: "p_right", damaged: false, galvanized: false },
            ],
            command_tokens: [],
          },
        },
      };

      render(
        <Board board={multiPlanetBoard} seatingOrder={["p1", "p2"]} overlayMode="ground_combat" />,
      );

      const overlayLeft = screen.getByTestId("ground-combat-overlay-p_left");
      const overlayRight = screen.getByTestId("ground-combat-overlay-p_right");

      expect(overlayLeft).toBeInTheDocument();
      expect(overlayRight).toBeInTheDocument();

      const planetLeft = screen.getByTestId("planet-p_left");
      const planetRight = screen.getByTestId("planet-p_right");
      const circleLeft = planetLeft.querySelector("circle");
      const circleRight = planetRight.querySelector("circle");

      expect(circleLeft).toBeInTheDocument();
      expect(circleRight).toBeInTheDocument();

      const cxLeft = parseFloat(circleLeft?.getAttribute("cx") ?? "0");
      const cyLeft = parseFloat(circleLeft?.getAttribute("cy") ?? "0");
      const cxRight = parseFloat(circleRight?.getAttribute("cx") ?? "0");
      const cyRight = parseFloat(circleRight?.getAttribute("cy") ?? "0");

      // Verify that planets have distinct X and Y coordinates (staggered diagonal)
      expect(cxLeft).not.toEqual(cxRight);
      expect(cyLeft).not.toEqual(cyRight);
      // Both numbers are written cleanly inside each planet
      expect(overlayLeft).toHaveTextContent("1");
      expect(overlayRight).toHaveTextContent("1");
    });

    it("displays tech benefits on specialty planets without top system summary", () => {
      const boardWithUpperTech: BoardView = {
        ...overlayBoard,
        map_tiles: [
          {
            system_id: "201",
            label: "Alpha System",
            q: 0,
            r: 0,
            planets: [
              {
                id: "planet_alpha",
                label: "Alpha Planet",
                resources: 3,
                influence: 1,
                tech_specialties: ["CYBERNETIC"], // Uppercase test
              },
            ],
          },
        ],
      };

      render(
        <Board
          board={boardWithUpperTech}
          seatingOrder={["p1", "p2"]}
          overlayMode="tech_benefits"
        />,
      );

      // System-level summary is removed as requested
      expect(screen.queryByTestId("tech-benefit-system-201")).not.toBeInTheDocument();

      // planet_alpha has cybernetic specialty badge on planet
      const techAlpha = screen.getByTestId("tech-benefit-planet-planet_alpha");
      expect(techAlpha).toBeInTheDocument();
      expect(techAlpha).toHaveTextContent("Cybernetic");
      expect(techAlpha).toHaveTextContent("🟡");

      // planet_beta has no tech specialty
      expect(screen.queryByTestId("tech-benefit-planet-planet_beta")).not.toBeInTheDocument();
    });

    it("displays overlay section in hover tooltip when an overlay is active", () => {
      render(<Board board={overlayBoard} seatingOrder={["p1", "p2"]} overlayMode="economy" />);

      // Hover over system 201
      const sysHex = screen.getByTestId("system-hex-201");
      fireEvent.mouseEnter(sysHex);

      const tooltip = screen.getByTestId("system-tooltip");
      expect(tooltip).toBeInTheDocument();

      const overlayTooltip = screen.getByTestId("system-tooltip-overlay");
      expect(overlayTooltip).toHaveTextContent("Economy Overlay");
      expect(overlayTooltip).toHaveTextContent("Ready:");
      expect(within(overlayTooltip).getByLabelText("Ready: 3 resources, 1 influence")).toBeInTheDocument();
    });
  });
});

describe("Board planet selection mode", () => {
  const planetChoice = {
    nonce: "nonce_mining",
    actor: "p1",
    prompt: "Mining Initiative: mine which planet",
    context: partialContext({
      subtype: "mining_initiative_pick_planet",
      source: { ActionCard: "mining_initiative" },
    }),
    options: [
      { id: "abyz", kind: "planet", label: "Abyz", payload: { planet: "abyz", system: "34" } },
      { id: "fria", kind: "planet", label: "Fria", payload: { planet: "fria", system: "34" } },
      {
        id: "mecatol_rex",
        kind: "planet",
        label: "Mecatol Rex",
        payload: { planet: "mecatol_rex" },
      },
    ],
  };

  it("draws planet reticles, keeps hexes non-targetable and dims other planets", () => {
    const board: BoardView = {
      ...mockBoard,
      systems: {
        ...mockBoard.systems,
        "34": {
          ...mockBoard.systems["34"],
          planets: {
            ...mockBoard.systems["34"].planets,
            loki: { planet_id: "loki", controlled_by: "p1", exhausted: false, attachments: [] },
          },
        },
      },
    };
    render(
      <Board
        board={board}
        seatingOrder={["p1", "p2"]}
        pendingChoice={planetChoice}
        viewerSeat="p1"
      />,
    );
    expect(screen.getByTestId("planet-target-reticle-abyz")).toBeInTheDocument();
    expect(screen.getByTestId("planet-target-reticle-mecatol_rex")).toBeInTheDocument();
    expect(screen.queryByTestId("activation-target-reticle")).not.toBeInTheDocument();
    expect(screen.getByTestId("system-hex-34")).not.toHaveAttribute("data-target-candidate");
    expect(screen.getByTestId("planet-loki")).toHaveAttribute("data-target-dimmed", "true");
    expect(screen.getByTestId("planet-loki")).not.toHaveAttribute("role");
  });

  it("selects a planet from a hex click only when it is the system's single candidate", () => {
    const onSelectTarget = vi.fn();
    const onSelectSystem = vi.fn();
    render(
      <Board
        board={mockBoard}
        seatingOrder={["p1", "p2"]}
        pendingChoice={planetChoice}
        viewerSeat="p1"
        onSelectTarget={onSelectTarget}
        onSelectSystem={onSelectSystem}
      />,
    );
    fireEvent.click(screen.getByTestId("system-hex-34"));
    expect(onSelectSystem).toHaveBeenCalledWith("34");
    expect(onSelectTarget).not.toHaveBeenCalled();

    fireEvent.click(screen.getByTestId("system-hex-18"));
    expect(onSelectTarget).toHaveBeenCalledWith("18", "mecatol_rex");

    fireEvent.click(screen.getByTestId("planet-fria"));
    expect(onSelectTarget).toHaveBeenLastCalledWith("34", "fria");
  });

  it("shows the clickable standard planets while a planet pick is pending, whatever the overlay", () => {
    const onSelectTarget = vi.fn();
    render(
      <Board
        board={mockBoard}
        seatingOrder={["p1", "p2"]}
        pendingChoice={planetChoice}
        viewerSeat="p1"
        overlayMode="economy"
        onSelectTarget={onSelectTarget}
      />,
    );
    expect(screen.queryByTestId("economy-overlay-34")).not.toBeInTheDocument();
    expect(screen.getByTestId("overlay-btn-economy")).toBeDisabled();
    fireEvent.click(screen.getByTestId("planet-abyz"));
    expect(onSelectTarget).toHaveBeenCalledWith("34", "abyz");
  });
});

describe("Board payment mode", () => {
  const payChoice = {
    nonce: "nonce_pay",
    actor: "p1",
    prompt: "pay 5 resources",
    context: partialContext({
      subtype: "pay_resources",
      outstanding: [{ kind: "Resources", amount: 5, paid: 0 }],
    }),
    options: [
      { id: "exhaust|fria", kind: "pay", label: "Fria", payload: { worth: 4, kind: "resources", planet_name: "Fria" } },
      { id: "decline", kind: "decline", label: "Cancel", payload: {} },
    ],
  };
  const board: BoardView = {
    ...mockBoard,
    systems: {
      ...mockBoard.systems,
      "34": {
        ...mockBoard.systems["34"],
        planets: {
          ...mockBoard.systems["34"].planets,
          fria: { planet_id: "fria", controlled_by: "p1", exhausted: false, attachments: [] },
        },
      },
    },
  };

  it("marks payable planets with their worth, dims the rest, and routes a click to the target", () => {
    const onSelectTarget = vi.fn();
    render(
      <Board
        board={board}
        seatingOrder={["p1", "p2"]}
        pendingChoice={payChoice}
        viewerSeat="p1"
        onSelectTarget={onSelectTarget}
      />,
    );
    expect(screen.getByTestId("payment-mark-fria")).toHaveAttribute("aria-label", "4 resources, also 0 influence");
    expect(screen.getByTestId("planet-fria")).toHaveAttribute("data-payment-staged", "false");
    expect(screen.getByTestId("planet-abyz")).toHaveAttribute("data-target-dimmed", "true");
    fireEvent.click(screen.getByTestId("planet-fria"));
    expect(onSelectTarget).toHaveBeenCalledWith("34", "fria");
  });

  it("does not open the system inspector when a payable planet is clicked", () => {
    const onSelectSystem = vi.fn();
    render(
      <Board
        board={board}
        seatingOrder={["p1", "p2"]}
        pendingChoice={payChoice}
        viewerSeat="p1"
        onSelectSystem={onSelectSystem}
        onSelectTarget={vi.fn()}
      />,
    );
    fireEvent.click(screen.getByTestId("planet-fria"));
    expect(onSelectSystem).not.toHaveBeenCalled();
  });

  it("shows a staged planet as staged", () => {
    render(
      <PaymentDraftProvider
        value={{
          draft: { planetIds: ["exhaust|fria"], tradeGoods: 0 },
          togglePlanet: vi.fn(),
          setTradeGoods: vi.fn(),
          setDraft: vi.fn(),
          reset: vi.fn(),
        }}
      >
        <Board board={board} seatingOrder={["p1", "p2"]} pendingChoice={payChoice} viewerSeat="p1" />
      </PaymentDraftProvider>,
    );
    expect(screen.getByTestId("planet-fria")).toHaveAttribute("data-payment-staged", "true");
    expect(screen.getByTestId("payment-mark-fria")).toHaveAttribute("aria-label", "Staged: 4 resources, also 0 influence");
    expect(screen.getByTestId("payment-mark-fria")).toHaveTextContent("✓ 4");
  });

  // Run s2-04-2345: while the shell paid a production's later build from the confirmed plan, the map
  // still offered "Exhaust planet Qucen'n" for the open question, a toggle no dialog could confirm.
  it("offers no planets while a production's later payment is answered from the confirmed plan", () => {
    const plan = (panelNonce: string): ProductionPaymentApi => ({
      plan: {
        actor: "p1",
        system: "34",
        costs: [2, 2],
        submitted: 2,
        mode: "auto",
        remaining: { planetIds: ["exhaust|fria"], tradeGoods: 0 },
        panelNonce,
        handledNonce: "nonce_pay",
      },
      confirm: vi.fn(),
      revert: vi.fn(),
      askEach: vi.fn(),
    });
    const view = (panelNonce: string) => (
      <ProductionPaymentProvider value={plan(panelNonce)}>
        <Board board={board} seatingOrder={["p1", "p2"]} pendingChoice={payChoice} viewerSeat="p1" />
      </ProductionPaymentProvider>
    );
    const { rerender } = render(view("nonce_earlier"));
    expect(screen.getByTestId("planet-fria")).not.toHaveAttribute("data-payment-staged");
    expect(screen.getByTestId("planet-fria")).not.toHaveAttribute("data-target-candidate");
    expect(screen.queryByTestId("payment-mark-fria")).toBeNull();

    // The question the player's own panel answers keeps its planets on the map.
    rerender(view("nonce_pay"));
    expect(screen.getByTestId("planet-fria")).toHaveAttribute("data-payment-staged", "false");
  });

  it("pinch with two pointers zooms the map group and the viewport opts out of page zoom", () => {
    render(<Board board={mockBoard} seatingOrder={["p1", "p2"]} />);
    const viewport = screen.getByTestId("board-viewport");
    expect(viewport.style.touchAction).toBe("none");
    const group = () => viewport.querySelector("svg > g")!.getAttribute("transform")!;
    const before = group();
    const ev = (id: number, x: number) => ({ pointerId: id, pointerType: "touch", clientX: x, clientY: 100, button: 0 });
    fireEvent.pointerDown(viewport, ev(1, 100));
    fireEvent.pointerDown(viewport, ev(2, 200));
    fireEvent.pointerMove(viewport, ev(2, 300));
    expect(group()).not.toBe(before);
    expect(group()).toMatch(/scale\(2(\.\d+)?\)/);
    fireEvent.pointerUp(viewport, ev(2, 300));
    fireEvent.pointerUp(viewport, ev(1, 100));
  });
});
