import { act } from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { HitAssignmentPanel } from "./HitAssignmentPanel.tsx";
import { onlyFighterOptions } from "../presentation/hitAssignment.ts";
import { SpaceCombatOverlay } from "./SpaceCombatOverlay.tsx";
import { InvasionOverlay } from "./InvasionOverlay.tsx";
import type {
  BoardView,
  PendingChoiceDto,
  PlacedUnitView,
} from "../protocol/types.ts";

const unit = (unit_type: string, damaged = false): PlacedUnitView => ({
  unit_type,
  owner: "seat_1",
  damaged,
});

const units = [
  unit("fighter"),
  unit("fighter"),
  unit("fighter"),
  unit("destroyer"),
  unit("dreadnought"),
  unit("carrier"),
];

function renderPanel(
  onSubmitPlan = vi.fn().mockResolvedValue(undefined),
  hits = 2,
) {
  render(
    <HitAssignmentPanel
      title="Assign hits"
      hits={hits}
      units={units}
      sustainTypes={new Set(["dreadnought"])}
      cargo={{ load: 3, capacity: 5 }}
      onSubmitPlan={onSubmitPlan}
    />,
  );
  return onSubmitPlan;
}

describe("HitAssignmentPanel", () => {
  it("groups simple ships and shows sustaining and capacity ships one by one", () => {
    renderPanel();
    expect(screen.getByTestId("hit-row-fighter|intact")).toHaveTextContent(
      "×3",
    );
    expect(screen.getByTestId("hit-row-fighter|intact")).toHaveTextContent(
      "takes up to 3 hits",
    );
    expect(screen.getByTestId("hit-row-dreadnought#1")).toHaveTextContent(
      "can sustain damage",
    );
    expect(screen.getByTestId("hit-row-carrier#1")).toHaveTextContent(
      "capacity 4",
    );
    expect(screen.getByTestId("hit-assignment-cargo")).toHaveTextContent(
      "Cargo in space: 3",
    );
  });

  it("submits nothing until confirmed, then sends the staged plan with sustains first", async () => {
    const onSubmitPlan = renderPanel();
    fireEvent.click(screen.getByTestId("hit-destroy-fighter|intact"));
    expect(screen.getByTestId("hit-confirm")).toBeDisabled();
    fireEvent.click(screen.getByTestId("hit-sustain-dreadnought#1"));
    expect(onSubmitPlan).not.toHaveBeenCalled();
    expect(screen.getByTestId("hit-destroy-destroyer|intact")).toBeDisabled();
    await act(async () => {
      fireEvent.click(screen.getByTestId("hit-confirm"));
    });
    expect(onSubmitPlan).toHaveBeenCalledWith([
      { kind: "sustain", unit: "dreadnought" },
      { kind: "destroy", unit: "fighter", damaged: false },
    ]);
  });

  it("shows what destroying a sustaining ship costs and offers only the sustain with one hit left", () => {
    renderPanel(undefined, 1);
    expect(screen.getByTestId("hit-row-dreadnought#1")).toHaveTextContent(
      "2 hits to destroy",
    );
    expect(screen.getByTestId("hit-sustain-dreadnought#1")).toBeEnabled();
    expect(screen.getByTestId("hit-destroy-dreadnought#1")).toBeDisabled();
  });

  it("stages a sustain and the destroy that follows as two hits, undone destroy first", async () => {
    const onSubmitPlan = renderPanel(undefined, 2);
    fireEvent.click(screen.getByTestId("hit-destroy-dreadnought#1"));
    expect(screen.getByTestId("hit-staged-dreadnought#1")).toHaveTextContent(
      "destroyed",
    );
    expect(screen.getByTestId("hit-assignment-remaining")).toHaveTextContent(
      "0 of 2 hits left",
    );
    fireEvent.click(screen.getByTestId("hit-remove-dreadnought#1"));
    expect(screen.getByTestId("hit-staged-dreadnought#1")).toHaveTextContent(
      "sustains",
    );
    fireEvent.click(screen.getByTestId("hit-destroy-dreadnought#1"));
    await act(async () => {
      fireEvent.click(screen.getByTestId("hit-confirm"));
    });
    expect(onSubmitPlan).toHaveBeenCalledWith([
      { kind: "sustain", unit: "dreadnought" },
      { kind: "destroy", unit: "dreadnought", damaged: true },
    ]);
  });

  it("moves a hit between ships before confirming", async () => {
    const onSubmitPlan = renderPanel(undefined, 1);
    fireEvent.click(screen.getByTestId("hit-destroy-fighter|intact"));
    fireEvent.click(screen.getByTestId("hit-remove-fighter|intact"));
    fireEvent.click(screen.getByTestId("hit-destroy-destroyer|intact"));
    await act(async () => {
      fireEvent.click(screen.getByTestId("hit-confirm"));
    });
    expect(onSubmitPlan).toHaveBeenCalledWith([
      { kind: "destroy", unit: "destroyer", damaged: false },
    ]);
  });

  it("warns when the staged losses strand cargo", () => {
    renderPanel(undefined, 1);
    fireEvent.click(screen.getByTestId("hit-destroy-carrier#1"));
    expect(screen.getByTestId("hit-assignment-cargo")).toHaveTextContent(
      "leave 2 without capacity",
    );
  });

  it("clears the staging and shows why after the server rejects the plan", async () => {
    const onSubmitPlan = renderPanel(
      vi.fn().mockRejectedValue(new Error("workflow interrupted")),
      1,
    );
    fireEvent.click(screen.getByTestId("hit-destroy-fighter|intact"));
    await act(async () => {
      fireEvent.click(screen.getByTestId("hit-confirm"));
    });
    expect(onSubmitPlan).toHaveBeenCalledTimes(1);
    expect(screen.getByTestId("hit-assignment-error")).toHaveTextContent(
      "workflow interrupted",
    );
    expect(screen.getByTestId("hit-staged-fighter|intact")).toHaveTextContent(
      "",
    );
    expect(screen.getByTestId("hit-confirm")).toBeDisabled();
  });
});

describe("anti-fighter barrage hit staging", () => {
  it("offers only fighters and never lets the hits fall on other ships", () => {
    render(
      <HitAssignmentPanel
        title="Assign barrage hits"
        hits={2}
        units={units}
        sustainTypes={new Set(["dreadnought"])}
        onlyFighters
        onSubmitPlan={vi.fn()}
      />,
    );
    expect(screen.getByTestId("hit-row-fighter|intact")).toBeInTheDocument();
    expect(
      screen.queryByTestId("hit-row-destroyer|intact"),
    ).not.toBeInTheDocument();
    expect(screen.queryByTestId("hit-row-carrier#1")).not.toBeInTheDocument();
    expect(
      screen.queryByTestId("hit-row-dreadnought#1"),
    ).not.toBeInTheDocument();
  });

  it("recognises a barrage decision that only offers fighters", () => {
    const fighter = {
      id: "destroy|0",
      kind: "casualty",
      label: "",
      payload: { unit: "fighter" },
    };
    const carrier = {
      id: "destroy|1",
      kind: "casualty",
      label: "",
      payload: { unit: "carrier" },
    };
    expect(onlyFighterOptions([fighter])).toBe(true);
    expect(onlyFighterOptions([fighter, carrier])).toBe(false);
    expect(onlyFighterOptions([])).toBe(false);
  });
});

const board: BoardView = {
  systems: {
    "18": {
      system_id: "18",
      command_tokens: [],
      planets: {},
      units: [
        { unit_type: "dreadnought", owner: "seat_1", damaged: false },
        { unit_type: "fighter", owner: "seat_1", damaged: false },
        { unit_type: "fighter", owner: "seat_1", damaged: false },
        { unit_type: "cruiser", owner: "seat_2", damaged: false },
      ],
    },
  },
  active_system: "18",
};

describe("space combat hit staging", () => {
  it("replaces per-click casualties with the panel and sends one casualty plan", async () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    const onSubmitBatch = vi.fn().mockResolvedValue(undefined);
    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "n1",
      prompt: "cancel a hit at 18",
      context: {
        subtype: "sustain_damage",
        target: { System: "18" },
        outstanding: [{ kind: "UnitsToRemove", amount: 2, paid: 0 }],
      },
      options: [
        {
          id: "sustain|0",
          kind: "sustain",
          label: "sustain damage on dreadnought",
          payload: { unit: "dreadnought" },
        },
        { id: "decline", kind: "decline", label: "take the hit" },
      ],
    };
    render(
      <SpaceCombatOverlay
        isOpen
        choice={choice}
        viewerSeat="seat_1"
        board={board}
        onSubmit={onSubmit}
        onClose={vi.fn()}
        onSubmitBatch={onSubmitBatch}
      />,
    );
    expect(screen.queryByTestId("decline-sustain-btn")).not.toBeInTheDocument();
    expect(screen.getByTestId("unit-row-dreadnought")).not.toHaveAttribute(
      "role",
      "button",
    );
    fireEvent.click(screen.getByTestId("hit-auto-assign"));
    await act(async () => {
      fireEvent.click(screen.getByTestId("hit-confirm"));
    });
    expect(onSubmit).not.toHaveBeenCalled();
    expect(onSubmitBatch).toHaveBeenCalledWith({
      kind: "casualties",
      steps: [
        { kind: "sustain", unit: "dreadnought" },
        { kind: "destroy", unit: "fighter", damaged: false },
      ],
    });
  });
});

describe("ground combat hit staging", () => {
  it("uses the same panel for ground combat casualties", async () => {
    const onSubmitBatch = vi.fn().mockResolvedValue(undefined);
    const ground: BoardView = {
      systems: {
        "18": {
          system_id: "18",
          command_tokens: [],
          planets: {},
          units: [
            {
              unit_type: "infantry",
              owner: "seat_1",
              damaged: false,
              planet: "jord",
            },
            {
              unit_type: "mech",
              owner: "seat_1",
              damaged: true,
              planet: "jord",
            },
            {
              unit_type: "infantry",
              owner: "seat_2",
              damaged: false,
              planet: "jord",
            },
          ],
        },
      },
      active_system: "18",
      invasion: {
        system_id: "18",
        invasion_seq: 1,
        invader: "seat_1",
        phase: "ground_battle",
        planets: ["jord"],
        current_planet: "jord",
        defender: "seat_2",
        ground_round: 1,
      },
    } as BoardView;
    const choice: PendingChoiceDto = {
      actor: "seat_1",
      nonce: "g1",
      prompt: "assign a hit on jord",
      context: {
        subtype: "assign_ground_casualty",
        target: { Planet: { system: "18", planet: "jord" } },
      },
      options: [
        {
          id: "destroy|0",
          kind: "ground_casualty",
          label: "destroy infantry",
          payload: { unit: "infantry", damaged: false },
        },
        {
          id: "destroy|1",
          kind: "ground_casualty",
          label: "destroy mech",
          payload: { unit: "mech", damaged: true },
        },
      ],
    };
    render(
      <InvasionOverlay
        board={ground}
        choice={choice}
        viewerSeat="seat_1"
        onSubmit={vi.fn()}
        onClose={vi.fn()}
        onSubmitBatch={onSubmitBatch}
      />,
    );
    fireEvent.click(screen.getByTestId("hit-destroy-infantry|intact"));
    await act(async () => {
      fireEvent.click(screen.getByTestId("hit-confirm"));
    });
    expect(onSubmitBatch).toHaveBeenCalledWith({
      kind: "casualties",
      steps: [{ kind: "destroy", unit: "infantry", damaged: false }],
    });
  });
});
