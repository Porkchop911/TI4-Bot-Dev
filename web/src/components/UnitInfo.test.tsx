import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { FactionInfoButton, MechInfoRow, UnitInfoButton } from "./UnitInfo.tsx";
import { SeatInfoProvider } from "../presentation/SeatInfoContext.tsx";
import { ProductionBuilderDrawer } from "./ProductionBuilderDrawer.tsx";
import { PlayerSheet } from "./PlayerSheet.tsx";
import type { PlayerView } from "../protocol/types.ts";

const player = (over: Partial<PlayerView>): PlayerView => ({
  id: "seat_1",
  faction: "sol",
  victory_points: 0,
  trade_goods: 0,
  commodities: 0,
  tactic_tokens: 3,
  fleet_tokens: 3,
  strategic_tokens: 2,
  passed: false,
  strategy_cards: [],
  exhausted_strategy_cards: [],
  technologies: [],
  exhausted_technologies: [],
  relics: [],
  exhausted_relics: [],
  action_cards_count: 0,
  secret_objectives_count: 0,
  leaders: {},
  ...over,
});

describe("InfoPopover behaviour (through the unit button)", () => {
  it("opens on keyboard focus and closes on Escape", () => {
    render(<UnitInfoButton unit="dreadnought" />);
    const trigger = screen.getByRole("button", { name: "Dreadnought I: unit details" });
    expect(screen.queryByTestId("unit-info-card")).toBeNull();
    fireEvent.focus(trigger);
    expect(screen.getByTestId("unit-info-card")).toBeInTheDocument();
    expect(screen.getByRole("dialog", { name: "Dreadnought I: unit details" })).toBeInTheDocument();
    fireEvent.keyDown(trigger, { key: "Escape" });
    expect(screen.queryByTestId("unit-info-card")).toBeNull();
  });

  it("opens on mouse hover and closes when the pointer leaves", () => {
    vi.useFakeTimers();
    try {
      render(<UnitInfoButton unit="cruiser" />);
      const trigger = screen.getByTestId("unit-info-cruiser");
      fireEvent.pointerEnter(trigger, { pointerType: "mouse" });
      expect(screen.getByTestId("unit-info-card")).toBeInTheDocument();
      fireEvent.pointerLeave(trigger, { pointerType: "mouse" });
      act(() => {
        vi.advanceTimersByTime(300);
      });
      expect(screen.queryByTestId("unit-info-card")).toBeNull();
    } finally {
      vi.useRealTimers();
    }
  });

  it("opens on a tap (click) and a click pins a hover-opened card", () => {
    vi.useFakeTimers();
    try {
      render(<UnitInfoButton unit="cruiser" />);
      const trigger = screen.getByTestId("unit-info-cruiser");
      fireEvent.click(trigger);
      expect(screen.getByTestId("unit-info-card")).toBeInTheDocument();
      fireEvent.click(trigger);
      expect(screen.queryByTestId("unit-info-card")).toBeNull();

      fireEvent.pointerEnter(trigger, { pointerType: "mouse" });
      fireEvent.click(trigger);
      fireEvent.pointerLeave(trigger, { pointerType: "mouse" });
      act(() => {
        vi.advanceTimersByTime(300);
      });
      expect(screen.getByTestId("unit-info-card")).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });

  it("renders the card in document.body so a scrolling dialog cannot clip it", () => {
    const { container } = render(<UnitInfoButton unit="cruiser" />);
    fireEvent.click(screen.getByTestId("unit-info-cruiser"));
    const card = screen.getByTestId("unit-info-cruiser-card");
    expect(card).toHaveAttribute("data-portal", "true");
    expect(card.parentElement).toBe(document.body);
    expect(container.contains(card)).toBe(false);
  });

  it("ignores touch pointer-enter so a tap is not a hover", () => {
    render(<UnitInfoButton unit="cruiser" />);
    fireEvent.pointerEnter(screen.getByTestId("unit-info-cruiser"), { pointerType: "touch" });
    expect(screen.queryByTestId("unit-info-card")).toBeNull();
  });
});

describe("UnitInfoButton content", () => {
  it("shows cost, combat with dice, move, capacity and abilities", () => {
    render(<UnitInfoButton unit="dreadnought" />);
    fireEvent.click(screen.getByTestId("unit-info-dreadnought"));
    const card = screen.getByTestId("unit-info-card");
    // Each fact is a term/definition pair; the colon between them is drawn by CSS.
    expect(card).toHaveTextContent("Cost4Combat5Move1Capacity1");
    expect(screen.getByRole("list", { name: "Abilities" })).toHaveTextContent("Bombardment 5");
    expect(screen.getByRole("list", { name: "Abilities" })).toHaveTextContent("Sustain damage");
  });

  it("applies the viewing seat's faction and owned upgrades", () => {
    render(
      <SeatInfoProvider
        players={[player({ faction: "hacan", technologies: ["dn2"] })]}
        viewerSeat="seat_1"
      >
        <UnitInfoButton unit="dreadnought" />
        <UnitInfoButton unit="flagship" />
      </SeatInfoProvider>,
    );
    fireEvent.click(screen.getByTestId("unit-info-dreadnought"));
    expect(screen.getByTestId("unit-info-card")).toHaveTextContent("Dreadnought II");
    fireEvent.click(screen.getByTestId("unit-info-dreadnought"));
    fireEvent.click(screen.getByTestId("unit-info-flagship"));
    expect(screen.getByTestId("unit-info-card")).toHaveTextContent("Wrath of Kenara");
    expect(screen.getByTestId("unit-info-card")).toHaveTextContent("spend 1 trade good");
  });

  it("degrades to a no-information card for a unit the content lacks", () => {
    render(<UnitInfoButton unit="mystery_unit" name="Mystery" />);
    fireEvent.click(screen.getByRole("button", { name: "Mystery: unit details" }));
    expect(screen.getByTestId("unit-info-empty")).toHaveTextContent("No information available");
  });
});

describe("mech info", () => {
  it("shows the viewer faction's mech and nothing for a faction without one", () => {
    const { rerender } = render(
      <SeatInfoProvider players={[player({ faction: "sol" })]} viewerSeat="seat_1">
        <MechInfoRow />
      </SeatInfoProvider>,
    );
    expect(screen.getByTestId("mech-info-row")).toHaveTextContent("ZS Thunderbolt M2");
    fireEvent.click(screen.getByTestId("mech-info"));
    expect(screen.getByTestId("unit-info-card")).toHaveTextContent("Orbital Drop");

    rerender(
      <SeatInfoProvider players={[player({ faction: "no_such_faction" })]} viewerSeat="seat_1">
        <MechInfoRow />
      </SeatInfoProvider>,
    );
    expect(screen.queryByTestId("mech-info-row")).toBeNull();
  });
});

describe("FactionInfoButton", () => {
  it("opens a card with abilities, note, flagship, mech, techs and leaders and their state", () => {
    render(
      <FactionInfoButton
        faction="sol"
        technologies={[]}
        leaders={{ solagent: "Readied", solhero: "Locked" }}
      />,
    );
    fireEvent.click(screen.getByRole("button", { name: "The Federation of Sol: faction details" }));
    const card = screen.getByTestId("faction-info-card");
    expect(screen.getAllByTestId("faction-ability").length).toBeGreaterThan(0);
    expect(screen.getByTestId("faction-note")).toHaveTextContent("Military Support");
    expect(card).toHaveTextContent("Genesis");
    expect(card).toHaveTextContent("ZS Thunderbolt M2");
    expect(screen.getByTestId("faction-starting-tech")).toBeInTheDocument();
    expect(screen.getByTestId("faction-leader-agent")).toHaveTextContent("readied");
    expect(screen.getByTestId("faction-leader-hero")).toHaveTextContent("locked");
    expect(screen.getByTestId("faction-leader-commander")).not.toHaveTextContent(/locked|readied/);
    expect(screen.getByTestId("faction-leader-commander")).toHaveTextContent("Unlock:");
  });

  it("degrades gracefully for a faction without catalog data", () => {
    render(<FactionInfoButton faction="no_such_faction" />);
    fireEvent.click(screen.getByRole("button", { name: "no_such_faction: faction details" }));
    expect(screen.getByTestId("faction-info-empty")).toHaveTextContent("No information available");
  });
});

describe("build options", () => {
  const choice = {
    actor: "seat_1",
    nonce: "n",
    prompt: "produce",
    context: { subtype: "produce_unit", outstanding: [{ amount: 3, paid: 0 }] },
    options: [
      {
        id: "build|dreadnought",
        kind: "produce",
        label: "Dreadnought",
        payload: { unit: "dreadnought", cost: 4, available_resources: 8 },
      },
      {
        id: "build|mech",
        kind: "produce",
        label: "Mech",
        payload: { unit: "mech", cost: 2, available_resources: 8 },
      },
    ],
  };

  it("puts an info card on every unit and the viewer's mech row in the production builder", () => {
    render(
      <SeatInfoProvider players={[player({ faction: "sol" })]} viewerSeat="seat_1">
        <ProductionBuilderDrawer
          choice={choice}
          viewerSeat="seat_1"
          onSubmit={vi.fn()}
          onClose={vi.fn()}
          isOpen
        />
      </SeatInfoProvider>,
    );
    fireEvent.click(screen.getByTestId("unit-info-dreadnought"));
    expect(screen.getByTestId("unit-info-card")).toHaveTextContent("Bombardment 5");
    fireEvent.click(screen.getByTestId("unit-info-dreadnought"));
    expect(screen.getByTestId("mech-info-row")).toHaveTextContent("ZS Thunderbolt M2");
    // Adding a unit still works next to the info button.
    fireEvent.click(screen.getByTestId("produce-unit-btn-build|dreadnought"));
    expect(screen.getByTestId("produce-count-build|dreadnought")).toHaveTextContent("1");
  });

  it("still renders without any seat information", () => {
    render(
      <ProductionBuilderDrawer
        choice={choice}
        viewerSeat="seat_1"
        onSubmit={vi.fn()}
        onClose={vi.fn()}
        isOpen
      />,
    );
    fireEvent.click(screen.getByTestId("unit-info-dreadnought"));
    expect(screen.getByTestId("unit-info-card")).toHaveTextContent("Dreadnought I");
    expect(screen.queryByTestId("mech-info-row")).toBeNull();
  });
});

describe("player sheet", () => {
  it("has a Faction button per player that opens that player's faction card", () => {
    const players = [
      player({ id: "p1", faction: "Federation of Sol", leaders: { solhero: "Locked" } }),
      player({ id: "p2", faction: "Barony of Letnev" }),
    ];
    render(<PlayerSheet players={players} userSeat="p1" />);
    const buttons = screen.getAllByTestId("faction-info-button");
    expect(buttons).toHaveLength(2);
    fireEvent.click(buttons[1]);
    expect(screen.getByTestId("faction-info-card")).toHaveTextContent("Barony of Letnev");
    expect(screen.getByTestId("faction-info-card")).toHaveTextContent("Arc Secundus");
  });
});
