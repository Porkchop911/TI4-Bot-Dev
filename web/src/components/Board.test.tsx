import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent } from "@testing-library/react";
import { Board, getPlayerColor } from "./Board.tsx";
import { BoardView } from "../protocol/types.ts";

const mockBoard: BoardView = {
  systems: {
    "18": {
      system_id: "18",
      coordinate: "<0, 0, 0>",
      tile_type: "normal",
      planets: {
        mecatol_rex: {
          planet_id: "mecatol_rex",
          controlled_by: null,
          exhausted: false,
          attachments: [],
        },
      },
      units: [
        { unit_type: "cruiser", owner: "p1", damaged: false },
        { unit_type: "infantry", owner: "p1", damaged: false },
      ],
      command_tokens: ["p1"],
    },
    "34": {
      system_id: "34",
      coordinate: "<1, 0, -1>",
      tile_type: "normal",
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
    expect(getPlayerColor("unusual-seat", ["unusual-seat", "another-seat"])).toBe("#E69F00");
    expect(getPlayerColor("another-seat", ["unusual-seat", "another-seat"])).toBe("#56B4E9");
    expect(getPlayerColor("absent-seat", ["unusual-seat"])).toBe("#94a3b8");
  });

  it("renders static map_tiles with anomalies, wormholes, and zoom controls", () => {
    const boardWithMap: BoardView = {
      systems: {
        "18": {
          system_id: "18",
          command_tokens: [],
          planets: {},
          units: [{ unit_type: "carrier", owner: "p1", damaged: false }],
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
    expect(screen.getByText("1/6")).toBeInTheDocument();

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

  it("renders activation target reticles during system activation mode", () => {
    const activationChoice = {
      nonce: "nonce_act",
      actor: "p1",
      prompt: "activate a system",
      context: { subtype: "activate_system" },
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
            { id: "first", label: "First", payload: { system: "18" } },
            { id: "second", label: "Second", payload: { system: "18" } },
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
      context: {
        subtype: "movement_step",
        target: { System: "18" },
      },
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
        { id: "done_moving", label: "Finish", kind: "decline" },
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
            { unit_type: "dreadnought", owner: "p1", damaged: false },
            { unit_type: "cruiser", owner: "p1", damaged: false },
            { unit_type: "infantry", owner: "p1", planet: "planet_alpha", damaged: false },
            { unit_type: "mech", owner: "p1", planet: "planet_alpha", damaged: false },
            { unit_type: "pds", owner: "p1", planet: "planet_alpha", damaged: false },
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
          units: [{ unit_type: "destroyer", owner: "p2", damaged: false }],
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
              { unit_type: "cruiser", owner: "p1", damaged: false },
              { unit_type: "cruiser", owner: "p2", damaged: false },
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
              { unit_type: "infantry", owner: "p1", planet: "p_left", damaged: false },
              { unit_type: "infantry", owner: "p2", planet: "p_right", damaged: false },
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
      expect(overlayTooltip).toHaveTextContent("Ready: 3 Res / 1 Inf");
    });
  });
});
