import { fireEvent, render, screen } from "@testing-library/react";
import { readFileSync } from "node:fs";
import { resolve } from "node:path";
import { beforeEach, describe, expect, it } from "vitest";
import { MINIMIZED_KEY, TurnActionBar, resetActionPaneMemory } from "./TurnActionBar.tsx";
import { deriveReadOnlyTurnBar, deriveTurnBar, type TurnBarModel } from "../presentation/turnBar.ts";
import type { PendingChoiceDto, PlayerView } from "../protocol/types.ts";

const tokens = { tactic: 3, fleet: 3, strategy: 2 };
const choice: PendingChoiceDto = {
  actor: "a",
  nonce: "n1",
  prompt: "action phase",
  options: [
    { id: "tactical", label: "take a tactical action", kind: "action" },
    { id: "pass", label: "pass", kind: "action" },
  ],
  details: { kind: "turn_menu", closing: false, tokens, strategy_cards: [], partners: [] },
};
const active = (): TurnBarModel => deriveTurnBar(choice, undefined)!;
const readonly = (): TurnBarModel => deriveReadOnlyTurnBar({ strategy_cards: [], exhausted_strategy_cards: [], passed: false } as unknown as PlayerView, "b");
const css = readFileSync(resolve(__dirname, "TurnActionBar.css"), "utf8");

const bar = () => screen.getByTestId("turn-action-bar");
const toggle = () => screen.getByTestId("turn-bar-minimize");
const show = (model: TurnBarModel) => (
  <TurnActionBar model={model} onSubmit={async () => undefined} />
);

describe("TurnActionBar minimize (phone)", () => {
  beforeEach(() => {
    window.localStorage.clear();
    resetActionPaneMemory();
  });

  it("starts expanded when no preference is stored, with the labelled control", () => {
    render(show(active()));
    expect(bar().dataset.minimized).toBe("false");
    expect(toggle().getAttribute("aria-expanded")).toBe("true");
    expect(toggle().getAttribute("aria-label")).toBe("Minimize actions");
    expect(toggle().getAttribute("aria-controls")).toBe(
      document.querySelector(".turn-bar__body")!.id,
    );
    expect(screen.getByTestId("turn-bar-tactical")).toBeTruthy();
  });

  it("collapses and expands, keeps the buttons mounted and stores the choice", () => {
    render(show(active()));
    fireEvent.click(toggle());
    expect(bar().dataset.minimized).toBe("true");
    expect(toggle().getAttribute("aria-expanded")).toBe("false");
    expect(toggle().getAttribute("aria-label")).toBe("Show actions");
    expect(screen.getByTestId("turn-bar-mini-text").textContent).toBe("Your turn — choose an action");
    expect(screen.getByTestId("turn-bar-tactical")).toBeTruthy();
    expect(window.localStorage.getItem(MINIMIZED_KEY)).toBe("1");
    fireEvent.click(toggle());
    expect(bar().dataset.minimized).toBe("false");
    expect(window.localStorage.getItem(MINIMIZED_KEY)).toBe("0");
  });

  it("restores the stored preference after a reload", () => {
    window.localStorage.setItem(MINIMIZED_KEY, "1");
    render(show(active()));
    expect(bar().dataset.minimized).toBe("true");
  });

  it("does not auto-expand on re-renders or a new decision of the same turn", () => {
    const { rerender } = render(show(active()));
    fireEvent.click(toggle());
    rerender(show({ ...active() }));
    rerender(show(deriveTurnBar({ ...choice }, undefined)!));
    expect(bar().dataset.minimized).toBe("true");
  });

  it("expands once when the turn passes from another seat to the viewer", () => {
    const { rerender } = render(show(readonly()));
    fireEvent.click(toggle());
    expect(bar().dataset.minimized).toBe("true");
    rerender(show(active()));
    expect(bar().dataset.minimized).toBe("false");
    expect(window.localStorage.getItem(MINIMIZED_KEY)).toBe("0");
    fireEvent.click(toggle());
    rerender(show({ ...active() }));
    expect(bar().dataset.minimized).toBe("true");
  });

  it("falls back to expanded when storage is blocked", () => {
    const orig = Storage.prototype.getItem;
    Storage.prototype.getItem = () => {
      throw new Error("blocked");
    };
    try {
      render(show(active()));
    } finally {
      Storage.prototype.getItem = orig;
    }
    expect(bar().dataset.minimized).toBe("false");
  });

  it("leaves the desktop layout alone: the control and the one-line text are phone-only", () => {
    expect(css).toMatch(/\.turn-bar__minimize,\s*\.turn-bar__mini-text \{\s*display: none;/);
    const phone = css.slice(
      css.indexOf("@media screen and (max-width: 720px), screen and (max-height: 500px) {\n  .turn-bar__head"),
    );
    expect(phone.startsWith("@media")).toBe(true);
    expect(phone).toContain('.turn-bar[data-minimized="true"]');
    expect(phone).toMatch(/height: 48px/);
    const before = css.slice(0, css.indexOf("/* Minimize control."));
    expect(before).not.toContain("data-minimized");
  });
});
