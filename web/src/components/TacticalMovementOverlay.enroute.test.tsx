import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, waitFor } from "@testing-library/react";
import { TacticalMovementOverlay } from "./TacticalMovementOverlay.tsx";
import type { BoardView, PendingChoiceDto } from "../protocol/types.ts";

// 95.1: a carrier also picks up from the systems it moves through. 24 (origin) - 30 - 18 (active).
const choice: PendingChoiceDto = {
  prompt: "movement",
  actor: "p1",
  nonce: "n1",
  context: { subtype: "movement_step", target: { System: "18" } },
  options: [
    {
      id: "move|24|0",
      label: "Carrier",
      kind: "move",
      payload: { origin: "24", unit: "carrier", capacity: 4 },
    },
    { id: "done_moving", label: "Finish Movement", kind: "decline" },
  ],
};

const tile = (system_id: string, q: number, r: number) => ({
  system_id,
  label: `#${system_id}`,
  q,
  r,
});
const sys = (
  id: string,
  units: BoardView["systems"][string]["units"],
  tokens: string[] = [],
) => ({
  system_id: id,
  command_tokens: tokens,
  planets: {},
  units,
});

const board = (mid: BoardView["systems"][string]): BoardView => ({
  map_tiles: [tile("24", 0, 0), tile("30", 1, 0), tile("18", 2, 0)],
  systems: {
    "24": sys("24", [{ owner: "p1", unit_type: "carrier", damaged: false }]),
    "30": mid,
    "18": sys("18", []),
  },
});

const infantryOnPlanet = {
  owner: "p1",
  unit_type: "infantry",
  planet: "mid",
  damaged: false,
};
const fighterInSpace = { owner: "p1", unit_type: "fighter", damaged: false };

const renderOverlay = (
  b: BoardView,
  onSubmitBatch = vi.fn().mockResolvedValue(undefined),
) => {
  render(
    <TacticalMovementOverlay
      choice={choice}
      board={b}
      activeSystemId="18"
      onSubmit={vi.fn().mockResolvedValue(undefined)}
      onSubmitBatch={onSubmitBatch}
      isOpen
      onClose={vi.fn()}
    />,
  );
  return onSubmitBatch;
};

describe("TacticalMovementOverlay en-route pickups (95.1)", () => {
  it("stages infantry on a planet of a system the carrier passes and plans the load", async () => {
    const onSubmitBatch = renderOverlay(
      board(sys("30", [infantryOnPlanet, infantryOnPlanet, fighterInSpace])),
    );
    expect(
      screen.getByTestId("rally-row-cargo-24-infantry-mid-via-30"),
    ).toHaveTextContent("On the way: #30");
    fireEvent.click(screen.getByTestId("rally-inc-24-carrier"));
    fireEvent.click(
      screen.getByTestId("rally-inc-cargo-24-infantry-mid-via-30"),
    );
    fireEvent.click(
      screen.getByTestId("rally-inc-cargo-24-infantry-mid-via-30"),
    );
    // only two infantry are there
    expect(
      screen.getByTestId("rally-inc-cargo-24-infantry-mid-via-30"),
    ).toBeDisabled();
    fireEvent.click(
      screen.getByTestId("rally-inc-cargo-24-fighter-space-via-30"),
    );
    fireEvent.click(screen.getByTestId("commit-moves-btn"));
    await waitFor(() =>
      expect(onSubmitBatch).toHaveBeenCalledExactlyOnceWith("18", [
        { kind: "move", origin: "24", unit: "carrier", damaged: false },
        {
          kind: "load",
          origin: "24",
          unit: "infantry",
          source: "mid",
          damaged: false,
        },
        {
          kind: "load",
          origin: "24",
          unit: "infantry",
          source: "mid",
          damaged: false,
        },
        {
          kind: "load",
          origin: "24",
          unit: "fighter",
          source: null,
          damaged: false,
        },
        { kind: "done_loading" },
        { kind: "done_moving" },
      ]),
    );
  });

  it("offers nothing from a system with the player's command token (95.5)", () => {
    renderOverlay(board(sys("30", [infantryOnPlanet], ["p1"])));
    expect(
      screen.queryByTestId("rally-row-cargo-24-infantry-mid-via-30"),
    ).toBeNull();
  });

  it("offers no cargo from an origin that holds the player's own command token (Dominus Orb, 95.5)", () => {
    const b = board(sys("30", []));
    b.systems["24"] = sys(
      "24",
      [
        { owner: "p1", unit_type: "carrier", damaged: false },
        { owner: "p1", unit_type: "fighter", damaged: false },
        { owner: "p1", unit_type: "infantry", planet: "home", damaged: false },
      ],
      ["p1"],
    );
    renderOverlay(b);
    expect(screen.getByTestId("rally-inc-24-carrier")).toBeInTheDocument();
    expect(screen.queryByTestId("rally-row-cargo-24-fighter-space")).toBeNull();
    expect(screen.queryByTestId("rally-row-cargo-24-infantry-home")).toBeNull();
  });

  it("still offers origin cargo without a token (control for the Dominus Orb case)", () => {
    const b = board(sys("30", []));
    b.systems["24"] = sys("24", [
      { owner: "p1", unit_type: "carrier", damaged: false },
      { owner: "p1", unit_type: "fighter", damaged: false },
    ]);
    renderOverlay(b);
    expect(screen.getByTestId("rally-row-cargo-24-fighter-space")).toBeInTheDocument();
  });

  it("offers nothing when the passed system has no own units", () => {
    renderOverlay(board(sys("30", [{ ...infantryOnPlanet, owner: "p2" }])));
    expect(
      screen.queryByTestId("rally-row-cargo-24-infantry-mid-via-30"),
    ).toBeNull();
  });

  it("needs a carrier staged: capacity gates the en-route cargo too", () => {
    renderOverlay(board(sys("30", [infantryOnPlanet])));
    expect(
      screen.getByTestId("rally-inc-cargo-24-infantry-mid-via-30"),
    ).toBeDisabled();
  });

  it("refuses a space pickup the engine would fill from another system's unit of the same kind", () => {
    const b = board(sys("30", [fighterInSpace]));
    b.systems["24"].units.push(fighterInSpace);
    renderOverlay(b);
    fireEvent.click(screen.getByTestId("rally-inc-24-carrier"));
    expect(
      screen.getByTestId("rally-inc-cargo-24-fighter-space-via-30"),
    ).toBeDisabled();
  });

  describe("mechs", () => {
    const mechOnPlanet = {
      owner: "p1",
      unit_type: "mech",
      planet: "mid",
      damaged: false,
    };
    const mechInSpace = { owner: "p1", unit_type: "mech", damaged: false };
    const stageCarrier = () =>
      fireEvent.click(screen.getByTestId("rally-inc-24-carrier"));
    const mechId = "cargo-24-mech-mid-via-30";

    it("stages a mech from a planet of a passed system and plans the load like infantry", async () => {
      const onSubmitBatch = renderOverlay(board(sys("30", [mechOnPlanet])));
      expect(screen.getByTestId(`rally-row-${mechId}`)).toHaveTextContent(
        "On the way: #30",
      );
      stageCarrier();
      fireEvent.click(screen.getByTestId(`rally-inc-${mechId}`));
      expect(screen.getByTestId(`rally-inc-${mechId}`)).toBeDisabled();
      fireEvent.click(screen.getByTestId("commit-moves-btn"));
      await waitFor(() =>
        expect(onSubmitBatch).toHaveBeenCalledExactlyOnceWith("18", [
          { kind: "move", origin: "24", unit: "carrier", damaged: false },
          {
            kind: "load",
            origin: "24",
            unit: "mech",
            source: "mid",
            damaged: false,
          },
          { kind: "done_loading" },
          { kind: "done_moving" },
        ]),
      );
    });

    it("stages a mech from the space area of a passed system", async () => {
      const onSubmitBatch = renderOverlay(board(sys("30", [mechInSpace])));
      stageCarrier();
      fireEvent.click(
        screen.getByTestId("rally-inc-cargo-24-mech-space-via-30"),
      );
      fireEvent.click(screen.getByTestId("commit-moves-btn"));
      await waitFor(() =>
        expect(onSubmitBatch).toHaveBeenCalledExactlyOnceWith("18", [
          { kind: "move", origin: "24", unit: "carrier", damaged: false },
          {
            kind: "load",
            origin: "24",
            unit: "mech",
            source: null,
            damaged: false,
          },
          { kind: "done_loading" },
          { kind: "done_moving" },
        ]),
      );
    });

    it("a mech and infantry share the hold: each takes one slot of four", async () => {
      const onSubmitBatch = renderOverlay(
        board(
          sys("30", [
            mechOnPlanet,
            infantryOnPlanet,
            infantryOnPlanet,
            infantryOnPlanet,
            infantryOnPlanet,
          ]),
        ),
      );
      stageCarrier();
      for (let i = 0; i < 3; i++) {
        fireEvent.click(
          screen.getByTestId("rally-inc-cargo-24-infantry-mid-via-30"),
        );
      }
      fireEvent.click(screen.getByTestId(`rally-inc-${mechId}`));
      // four slots used: nothing more fits
      expect(
        screen.getByTestId("rally-inc-cargo-24-infantry-mid-via-30"),
      ).toBeDisabled();
      fireEvent.click(screen.getByTestId("commit-moves-btn"));
      await waitFor(() => expect(onSubmitBatch).toHaveBeenCalled());
      const steps = onSubmitBatch.mock.calls[0][1];
      expect(
        steps.filter((x: { kind: string; unit?: string }) => x.kind === "load"),
      ).toHaveLength(4);
      expect(
        steps.filter((x: { unit?: string }) => x.unit === "mech"),
      ).toHaveLength(1);
    });

    it("a mech uses a slot: a full hold of infantry blocks it", () => {
      renderOverlay(
        board(
          sys("30", [
            mechOnPlanet,
            infantryOnPlanet,
            infantryOnPlanet,
            infantryOnPlanet,
            infantryOnPlanet,
          ]),
        ),
      );
      stageCarrier();
      for (let i = 0; i < 4; i++) {
        fireEvent.click(
          screen.getByTestId("rally-inc-cargo-24-infantry-mid-via-30"),
        );
      }
      expect(screen.getByTestId(`rally-inc-${mechId}`)).toBeDisabled();
    });

    it("needs a staged carrier", () => {
      renderOverlay(board(sys("30", [mechOnPlanet])));
      expect(screen.getByTestId(`rally-inc-${mechId}`)).toBeDisabled();
    });

    it("offers no mech from a system with the player's command token (95.5)", () => {
      renderOverlay(board(sys("30", [mechOnPlanet], ["p1"])));
      expect(screen.queryByTestId(`rally-row-${mechId}`)).toBeNull();
    });

    describe("Argent mech rides free (MovementHooks::free_cargo)", () => {
      const argent = {
        owner: "p1",
        unit_type: "argent_mech",
        planet: "mid",
        damaged: false,
      };
      const argentId = "cargo-24-argent_mech-mid-via-30";

      it("is stageable as a fifth unit on a hold of four", async () => {
        const onSubmitBatch = renderOverlay(
          board(
            sys("30", [
              argent,
              infantryOnPlanet,
              infantryOnPlanet,
              infantryOnPlanet,
              infantryOnPlanet,
            ]),
          ),
        );
        stageCarrier();
        for (let i = 0; i < 4; i++) {
          fireEvent.click(
            screen.getByTestId("rally-inc-cargo-24-infantry-mid-via-30"),
          );
        }
        expect(
          screen.getByTestId("rally-inc-cargo-24-infantry-mid-via-30"),
        ).toBeDisabled();
        expect(screen.getByTestId(`rally-inc-${argentId}`)).not.toBeDisabled();
        fireEvent.click(screen.getByTestId(`rally-inc-${argentId}`));
        fireEvent.click(screen.getByTestId("commit-moves-btn"));
        await waitFor(() => expect(onSubmitBatch).toHaveBeenCalled());
        const loads = onSubmitBatch.mock.calls[0][1].filter(
          (x: { kind: string }) => x.kind === "load",
        );
        expect(loads).toHaveLength(5);
      });

      it("the full plan of five loads ends the hold without a done_loading", async () => {
        const onSubmitBatch = renderOverlay(
          board(
            sys("30", [
              argent,
              infantryOnPlanet,
              infantryOnPlanet,
              infantryOnPlanet,
              infantryOnPlanet,
            ]),
          ),
        );
        stageCarrier();
        for (let i = 0; i < 4; i++) {
          fireEvent.click(
            screen.getByTestId("rally-inc-cargo-24-infantry-mid-via-30"),
          );
        }
        fireEvent.click(screen.getByTestId(`rally-inc-${argentId}`));
        fireEvent.click(screen.getByTestId("commit-moves-btn"));
        await waitFor(() => expect(onSubmitBatch).toHaveBeenCalled());
        const kinds = onSubmitBatch.mock.calls[0][1].map(
          (x: { kind: string }) => x.kind,
        );
        expect(kinds).toEqual([
          "move",
          "load",
          "load",
          "load",
          "load",
          "load",
          "done_moving",
        ]);
      });

      it("leaving the free mech behind closes the full hold only after it is declined", async () => {
        const onSubmitBatch = renderOverlay(
          board(
            sys("30", [
              argent,
              infantryOnPlanet,
              infantryOnPlanet,
              infantryOnPlanet,
              infantryOnPlanet,
            ]),
          ),
        );
        stageCarrier();
        for (let i = 0; i < 4; i++) {
          fireEvent.click(
            screen.getByTestId("rally-inc-cargo-24-infantry-mid-via-30"),
          );
        }
        fireEvent.click(screen.getByTestId("commit-moves-btn"));
        await waitFor(() => expect(onSubmitBatch).toHaveBeenCalled());
        const kinds = onSubmitBatch.mock.calls[0][1].map(
          (x: { kind: string }) => x.kind,
        );
        // the hold stays open to the free rider, so loading must be ended explicitly
        expect(kinds).toEqual([
          "move",
          "load",
          "load",
          "load",
          "load",
          "done_loading",
          "done_moving",
        ]);
      });

      it("still needs a staged hold", () => {
        renderOverlay(board(sys("30", [argent])));
        expect(screen.getByTestId(`rally-inc-${argentId}`)).toBeDisabled();
      });
    });
  });
});
