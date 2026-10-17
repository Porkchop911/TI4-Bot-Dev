import { fireEvent, render, screen, waitFor, within } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { ChoiceRendererDispatcher } from "./GameShell.tsx";
import { InvasionEffectCard } from "./InvasionOverlay.tsx";
import type { BoardView, PendingChoiceDto } from "../protocol/types.ts";
import {
  PlanetInfluenceContext,
  PlanetResourcesContext,
} from "../presentation/PlanetResourcesContext.tsx";
import { deriveActorTargetHighlights } from "../presentation/boardPresentation.ts";
import { partialContext } from "../test/wire.ts";

const board: BoardView = {
  systems: {
    "18": {
      system_id: "18",
      command_tokens: [],
      planets: { mr: { planet_id: "mr", controlled_by: null, exhausted: false } },
      units: [{ owner: "p1", unit_type: "infantry", planet: "mr", damaged: false, galvanized: false }],
    },
    "25": {
      system_id: "25",
      command_tokens: [],
      planets: {
        meer: { planet_id: "meer", controlled_by: "p1", exhausted: false },
        arinam: { planet_id: "arinam", controlled_by: "p1", exhausted: false },
      },
      units: [],
    },
  },
  invasion: {
    system_id: "18",
    invasion_seq: 3,
    invader: "p1",
    phase: "committing",
    planets: ["mr"],
    current_planet: "mr",
    defender: null,
    ground_round: 0,
    odds_context: {},
  },
} as BoardView;

// The custodians' six influence, as the engine asks for it after "remove it for a victory point".
const custodiansPayment: PendingChoiceDto = {
  actor: "p1",
  nonce: "pay-custodians",
  prompt: "pay 6 more influence",
  context: partialContext({
    subtype: "pay_influence",
    invasion_seq: 3,
    outstanding: [{ kind: "Influence", amount: 6, paid: 0 }],
  }),
  options: [
    { id: "exhaust|meer", kind: "pay", label: "exhaust meer for 4 influence", payload: { worth: 4, kind: "influence", source: "influence", owed: 6 } },
    { id: "exhaust|arinam", kind: "pay", label: "exhaust arinam for 2 influence", payload: { worth: 2, kind: "influence", source: "influence", owed: 6 } },
  ],
};

function dispatch(choice: PendingChoiceDto, viewerSeat = "p1") {
  const resources = new Map([
    ["meer", 0],
    ["arinam", 1],
  ]);
  const influence = new Map([
    ["meer", 4],
    ["arinam", 2],
  ]);
  return render(
    <PlanetResourcesContext.Provider value={resources}>
      <PlanetInfluenceContext.Provider value={influence}>
        <ChoiceRendererDispatcher
          choice={choice}
          viewerSeat={viewerSeat}
          boardView={board}
          onSubmit={vi.fn().mockResolvedValue(undefined)}
          isMinimized={false}
          onMinimizedChange={vi.fn()}
          players={{ p1: { trade_goods: 0 } as never }}
        />
      </PlanetInfluenceContext.Provider>
    </PlanetResourcesContext.Provider>,
  );
}

describe("payments inside an invasion", () => {
  it("pays the custodians with the payment panel: both values per planet and the total", () => {
    dispatch(custodiansPayment);
    expect(screen.queryByTestId("invasion-effect-card")).not.toBeInTheDocument();
    const meer = screen.getByTestId("planet-card-exhaust|meer-values");
    expect(meer).toHaveTextContent("0");
    expect(meer).toHaveTextContent("4");
    const arinam = screen.getByTestId("planet-card-exhaust|arinam-values");
    expect(arinam).toHaveTextContent("1");
    expect(arinam).toHaveTextContent("2");
    expect(screen.getByTestId("confirm-payment-btn")).toHaveTextContent("Stage 6 more to pay");
    fireEvent.click(screen.getByTestId("auto-pay-btn"));
    expect(screen.getByTestId("confirm-payment-btn")).not.toBeDisabled();
  });

  it("still shows the invasion overlay to the other seats while the invader pays", () => {
    dispatch(custodiansPayment, "p2");
    expect(screen.getByTestId("invasion-overlay")).toBeInTheDocument();
    expect(screen.queryByTestId("planet-card-exhaust|meer-values")).not.toBeInTheDocument();
  });

  it("rings the payable planets on the map during an invasion", () => {
    const highlights = deriveActorTargetHighlights(custodiansPayment, "p1", board);
    expect(highlights.targetMode).toBe("payment");
  });
});

describe("producing as a consequence of invasion", () => {
  const freelancers: PendingChoiceDto = {
    actor: "p1",
    nonce: "frln",
    prompt: "Freelancers: produce one unit in 25",
    context: partialContext({ subtype: "produce_one_unit", optional: true, target: { System: "25" } }),
    options: [
      { id: "build|destroyer|1", kind: "produce", label: "produce 1x destroyer for 1", payload: { unit: "destroyer", count: 1, cost: 1, system: "25" } },
      { id: "build|cruiser|1", kind: "produce", label: "produce 1x cruiser for 2", payload: { unit: "cruiser", count: 1, cost: 2, system: "25" } },
      { id: "decline", kind: "decline", label: "Decline", payload: {} },
    ],
  };

  it("offers every unit by name and cost and a decline that builds nothing", async () => {
    const submit = vi.fn().mockResolvedValue(undefined);
    render(<InvasionEffectCard choice={freelancers} onSubmit={submit} />);
    const card = screen.getByTestId("invasion-effect-card");
    expect(within(card).getByRole("button", { name: /Produce Destroyer/ })).toHaveTextContent("cost 1");
    const decline = screen.getByTestId("invasion-effect-decline");
    expect(decline).toHaveTextContent("Decline: build nothing");
    fireEvent.click(decline);
    await waitFor(() => expect(submit).toHaveBeenCalledWith("decline"));
  });

  it("names the exploration card that grants the build", () => {
    render(<InvasionEffectCard choice={freelancers} onSubmit={vi.fn()} />);
    expect(screen.getByTestId("invasion-effect-card")).toHaveTextContent("Freelancers");
  });
});
