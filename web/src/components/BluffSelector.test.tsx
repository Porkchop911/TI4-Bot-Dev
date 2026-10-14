import React from "react";
import { fireEvent, render, screen } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { describe, expect, it, vi, beforeEach } from "vitest";
import {
  BluffControls,
  BluffHoldBar,
  BluffSelector,
  bluffDisabledReason,
} from "./BluffSelector.tsx";
import { BLUFF_TRIGGER_IDS, readDeclaredTriggers, writeDeclaredTriggers } from "../protocol/bluffTriggers.ts";
import { ReactionIntentStateMsg, PROTOCOL_VERSION } from "../protocol/types.ts";

const intent = (over: Partial<ReactionIntentStateMsg> = {}): ReactionIntentStateMsg => ({
  protocol_version: PROTOCOL_VERSION,
  game_id: "g",
  triggers: [],
  max_triggers: 3,
  eligible: true,
  budget_used_up: false,
  holding: false,
  ...over,
});

const controls = (over: Partial<BluffControls> = {}): BluffControls => ({
  intent: intent(),
  round: 1,
  onChange: vi.fn(),
  stored: [],
  ...over,
});

const box = (id: string) => screen.getByTestId(`bluff-trigger-${id}`) as HTMLInputElement;

describe("BluffSelector", () => {
  beforeEach(() => localStorage.clear());

  it("sends the picked moments as one whole set only when declared", () => {
    const c = controls();
    render(<BluffSelector controls={c} cardsCount={2} neverMode={false} />);
    expect((screen.getByTestId("bluff-declare") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(box("agenda"));
    fireEvent.click(box("movement"));
    expect(c.onChange).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("bluff-declare"));
    expect(c.onChange).toHaveBeenCalledTimes(1);
    expect(c.onChange).toHaveBeenLastCalledWith(["agenda", "movement"]);
    expect(screen.getByTestId("bluff-selector").dataset.state).toBe("open");
  });

  it("stops at the server's limit of declared moments", () => {
    render(
      <BluffSelector
        controls={controls({ intent: intent({ triggers: ["agenda", "movement", "production"] }) })}
        cardsCount={2}
        neverMode={false}
      />,
    );
    expect(box("agenda").disabled).toBe(false);
    expect(box("space_combat").disabled).toBe(true);
  });

  it("is off, with the reason, for a seat with no action cards", () => {
    render(<BluffSelector controls={controls()} cardsCount={0} neverMode={false} />);
    expect(screen.getByTestId("bluff-selector").dataset.state).toBe("disabled");
    expect(screen.getByTestId("bluff-disabled-reason").textContent).toMatch(/no action cards/);
    expect(BLUFF_TRIGGER_IDS.every((id) => box(id).disabled)).toBe(true);
  });

  it("is off, with the reason, for a seat that set a card to Never offer", () => {
    render(<BluffSelector controls={controls()} cardsCount={3} neverMode />);
    expect(screen.getByTestId("bluff-disabled-reason").textContent).toMatch(/Never offer/);
    expect(box("agenda").disabled).toBe(true);
  });

  it("locks changes until the round the server names, but still lets the seat clear", () => {
    const c = controls({ intent: intent({ triggers: ["agenda"], locked_until_round: 3 }), round: 2 });
    render(<BluffSelector controls={c} cardsCount={2} neverMode={false} />);
    expect(screen.getByTestId("bluff-selector").dataset.state).toBe("locked");
    expect(screen.getByTestId("bluff-locked-reason").textContent).toMatch(/round 3/);
    expect(box("production").disabled).toBe(true);
    expect((screen.getByTestId("bluff-declare") as HTMLButtonElement).disabled).toBe(true);
    fireEvent.click(screen.getByTestId("bluff-clear"));
    expect(c.onChange).toHaveBeenCalledWith([]);
  });

  it("unlocks once the round has come", () => {
    render(
      <BluffSelector
        controls={controls({ intent: intent({ triggers: ["agenda"], locked_until_round: 3 }), round: 3 })}
        cardsCount={2}
        neverMode={false}
      />,
    );
    expect(box("production").disabled).toBe(false);
  });

  it("tells only this seat that its bluff budget is used up", () => {
    render(
      <BluffSelector
        controls={controls({ intent: intent({ budget_used_up: true }) })}
        cardsCount={2}
        neverMode={false}
      />,
    );
    expect(screen.getByTestId("bluff-budget-used-up").textContent).toMatch(/budget used up/i);
  });

  it("uses what this browser stored until the server has answered", () => {
    render(<BluffSelector controls={controls({ intent: null, stored: ["agenda"] })} cardsCount={2} neverMode={false} />);
    expect(box("agenda").checked).toBe(true);
  });

  it("explains a server refusal in the same words", () => {
    expect(
      bluffDisabledReason(
        { intent: intent({ eligible: false, ineligible_reason: "only a human seat can bluff" }), round: 1 },
        2,
        false,
      ),
    ).toMatch(/only a human seat/);
  });
});

describe("BluffHoldBar", () => {
  it("shows the Pass control only during a hold", () => {
    const pass = vi.fn();
    const { rerender } = render(<BluffHoldBar holding={false} onPass={pass} />);
    expect(screen.queryByTestId("bluff-hold-bar")).toBeNull();
    rerender(<BluffHoldBar holding onPass={pass} />);
    fireEvent.click(screen.getByTestId("bluff-pass"));
    expect(pass).toHaveBeenCalled();
  });
});

describe("declared triggers in this browser", () => {
  beforeEach(() => localStorage.clear());

  it("are kept per game and seat, and unknown ids are dropped", () => {
    writeDeclaredTriggers("g1", "p1", ["movement", "agenda"]);
    expect(readDeclaredTriggers("g1", "p1")).toEqual(["movement", "agenda"]);
    expect(readDeclaredTriggers("g1", "p2")).toEqual([]);
    expect(readDeclaredTriggers("g2", "p1")).toEqual([]);
    localStorage.setItem("bluff_triggers:g1:p1", JSON.stringify(["agenda", "bogus"]));
    expect(readDeclaredTriggers("g1", "p1")).toEqual(["agenda"]);
    localStorage.setItem("bluff_triggers:g1:p1", "not json");
    expect(readDeclaredTriggers("g1", "p1")).toEqual([]);
    writeDeclaredTriggers("g1", "p1", []);
    expect(localStorage.getItem("bluff_triggers:g1:p1")).toBeNull();
  });

  it("match the server's trigger ids", () => {
    const rust = readFileSync("../crates/ti4-server/src/session/bluff.rs", "utf8");
    const serverIds = [...rust.matchAll(/TriggerGroup \{\s*id: "([a-z_]+)"/g)].map((m) => m[1]);
    expect([...serverIds].sort()).toEqual([...BLUFF_TRIGGER_IDS].sort());
  });
});

// Keeps React imported for the JSX transform in older configs.
void React;
