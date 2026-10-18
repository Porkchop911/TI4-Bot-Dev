import { cleanup, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, describe, expect, it } from "vitest";
import { ActiveEffectsStrip } from "./ActiveEffectsStrip.tsx";
import type { ActiveEffectDto } from "../protocol/types.ts";

// As the engine words them (`ti4_host::view::effects`).
const card = (alias: string) => ({ text: "{0}", refs: { 0: { kind: "action_card" as const, card: alias } } });
const effects: ActiveEffectDto[] = [
  {
    seat: "p2",
    effect: "move_bonus",
    until: "tactical_action",
    card: "fs1",
    amount: 1,
    title: card("fs1"),
    summary: { text: "+1 move for all ships", refs: {} },
    expiry: "until the end of this tactical action",
  },
  {
    seat: "p1",
    effect: "combat_roll_bonus",
    until: "combat_round",
    card: "mb1",
    amount: 1,
    title: card("mb1"),
    summary: { text: "+1 to all combat rolls", refs: {} },
    expiry: "until the end of this combat round",
  },
];

afterEach(cleanup);

describe("ActiveEffectsStrip", () => {
  it("renders nothing while no card effect lingers", () => {
    const { container } = render(<ActiveEffectsStrip effects={[]} viewerSeat="p1" />);
    expect(container.firstChild).toBeNull();
    expect(document.documentElement.dataset.effectsStrip).toBeUndefined();
    const none = render(<ActiveEffectsStrip effects={undefined} viewerSeat="p1" />);
    expect(none.container.firstChild).toBeNull();
  });

  it("lists one chip per effect, the viewer's own first, and publishes its height", () => {
    render(<ActiveEffectsStrip effects={effects} viewerSeat="p1" />);
    const chips = screen.getAllByTestId("active-effect-chip");
    expect(chips.map((chip) => chip.textContent)).toEqual(["Morale Boost", "Flank Speed"]);
    expect(chips[0].dataset.mine).toBe("true");
    expect(document.documentElement.dataset.effectsStrip).toBe("on");
    expect(document.documentElement.style.getPropertyValue("--effects-strip-height")).toMatch(/px$/);
    expect(screen.queryByTestId("active-effects-details")).toBeNull();
  });

  it("opens the text and the expiry of each effect", () => {
    render(<ActiveEffectsStrip effects={effects} viewerSeat="p1" />);
    fireEvent.click(screen.getByTestId("active-effects-toggle"));
    const rows = screen.getAllByTestId("active-effect-row");
    expect(rows).toHaveLength(2);
    expect(rows[0].textContent).toContain("+1 to all combat rolls");
    expect(rows[0].textContent).toContain("until the end of this combat round");
    expect(rows[1].textContent).toContain("until the end of this tactical action");
  });

  it("drops an effect that the engine sent no words for", () => {
    render(
      <ActiveEffectsStrip
        effects={[
          {
            seat: "p1",
            effect: "from_the_future",
            until: "agenda",
            title: { text: "", refs: {} },
            summary: { text: "", refs: {} },
            expiry: "",
          },
        ]}
        viewerSeat="p1"
      />,
    );
    expect(screen.queryByTestId("active-effects-strip")).toBeNull();
  });

  it("clears the document marks when the last effect ends", () => {
    const { rerender } = render(<ActiveEffectsStrip effects={effects} viewerSeat="p1" />);
    rerender(<ActiveEffectsStrip effects={[]} viewerSeat="p1" />);
    expect(document.documentElement.dataset.effectsStrip).toBeUndefined();
  });
});
