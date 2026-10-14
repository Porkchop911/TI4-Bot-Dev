import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { TurnRedoBar } from "./TurnRedoBar.tsx";
import type { TurnRedoStatus } from "../protocol/turnRedo.ts";
import { PlayerIdentityProvider } from "../presentation/PlayerIdentity.tsx";
import { BoardPrepSlotContext } from "../presentation/BoardPrepSlot.tsx";
import type { LobbyDto } from "../protocol/types.ts";
import { WIDE_LAYOUT_QUERY } from "../hooks/useCompactLayout.ts";

const lobby: LobbyDto = {
  game_id: "g",
  phase: "running",
  lobby_version: 1,
  host_player_id: "p1",
  slots: [
    { slot_id: "s1", position: 1, occupant: "p1", nickname: "Ana", connected: true, can_take_over: false },
    { slot_id: "s2", position: 2, occupant: "p2", nickname: "Bo", connected: true, can_take_over: false },
  ],
};

const base: TurnRedoStatus = {
  seat: "p2",
  requested_by: "p2",
  turns_back: 1,
  redo_count: 1,
  stage: "new_turn",
  original_decisions: 120,
  rewound_to: 101,
  turn_complete: false,
  handoff_len: null,
  outcome: null,
  can_control: true,
};

const handoff: TurnRedoStatus = {
  ...base,
  stage: "auto_played",
  handoff_len: 106,
  outcome: { kept: 3, tail_total: 3, stop: { kind: "handoff", seat: "p2" }, asking_seat: "p2" },
};

const conflict: TurnRedoStatus = {
  ...base,
  stage: "auto_played",
  handoff_len: 106,
  outcome: {
    kept: 4,
    tail_total: 9,
    stop: {
      kind: "conflict",
      conflict: {
        original_cursor: 108,
        kind: "reserved_card",
        seat: "p1",
        prompt: "explore the planet",
        detail: "…",
        deck: "exploration:hazardous",
        card: "gamma_wormhole",
        recipient: "p1",
      },
    },
    asking_seat: "p1",
  },
};

const handlers = { onAutoplay: vi.fn(), onRestore: vi.fn(), onKeep: vi.fn() };

function ui(status: TurnRedoStatus | null, busy: null | "autoplay" = null, error: string | null = null) {
  return (
    <PlayerIdentityProvider lobby={lobby} seatingOrder={["p1", "p2"]}>
      <TurnRedoBar status={status} busy={busy} error={error} {...handlers} />
    </PlayerIdentityProvider>
  );
}

const original = window.matchMedia;
/** `matches` answers the compact query; `wide` the "wider than 720px" one (a phone in landscape). */
function mockMatchMedia(matches: boolean, wide = false) {
  const listeners = new Set<() => void>();
  const state = { matches };
  window.matchMedia = ((query: string) => ({
    get matches() {
      return query === WIDE_LAYOUT_QUERY ? wide : state.matches;
    },
    media: query,
    addEventListener: (_: string, cb: () => void) => listeners.add(cb),
    removeEventListener: (_: string, cb: () => void) => listeners.delete(cb),
  })) as unknown as typeof window.matchMedia;
  return {
    set(next: boolean) {
      state.matches = next;
      act(() => listeners.forEach((cb) => cb()));
    },
  };
}

describe("TurnRedoBar on a phone (compact layout)", () => {
  beforeEach(() => {
    mockMatchMedia(true);
  });
  afterEach(() => {
    window.matchMedia = original;
  });

  it("shows only a progress pill while the round replays", () => {
    render(ui({ ...base, turn_complete: true }, "autoplay"));
    const bar = screen.getByTestId("turn-redo-bar");
    expect(bar).toHaveClass("turn-redo--compact");
    expect(bar).toHaveAttribute("data-state", "replaying");
    expect(bar).toHaveAttribute("data-expanded", "false");
    expect(bar).toHaveAttribute("aria-busy", "true");
    const label = screen.getByRole("status");
    expect(label).toHaveAttribute("aria-live", "polite");
    expect(label.textContent).toBe("Replaying… up to Bo's next turn");
    expect(screen.queryByTestId("turn-redo-sheet")).toBeNull();
    expect(screen.queryByTestId("turn-redo-restore")).toBeNull();
    expect(screen.getByTestId("turn-redo-toggle")).toHaveAttribute("aria-expanded", "false");
  });

  it("opens and closes the panel on tap without moving focus", () => {
    render(ui(base));
    const toggle = screen.getByTestId("turn-redo-toggle");
    expect(screen.queryByTestId("turn-redo-sheet")).toBeNull();
    expect(document.activeElement).not.toBe(toggle);
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    expect(toggle).toHaveAccessibleName("Close the turn redo panel");
    expect(screen.getByTestId("turn-redo-bar")).toHaveAttribute("data-expanded", "true");
    expect(toggle).toHaveAttribute("aria-controls", screen.getByTestId("turn-redo-sheet").id);
    fireEvent.click(screen.getByTestId("turn-redo-restore"));
    expect(handlers.onRestore).toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("turn-redo-close"));
    expect(screen.getByTestId("turn-redo-bar")).toHaveAttribute("data-expanded", "false");
    expect(screen.queryByTestId("turn-redo-sheet")).toBeNull();
  });

  it("opens by itself only when the player has to decide", () => {
    const { rerender } = render(ui(base));
    expect(screen.getByTestId("turn-redo-bar")).toHaveAttribute("data-expanded", "false");
    rerender(ui(handoff));
    expect(screen.getByTestId("turn-redo-bar")).toHaveAttribute("data-expanded", "true");
    expect(screen.getByTestId("turn-redo-keep").textContent).toBe("Keep this timeline");
    expect(screen.getByTestId("turn-redo-restore")).toBeVisible();
    rerender(ui(conflict));
    expect(screen.getByTestId("turn-redo-bar")).toHaveAttribute("data-expanded", "true");
    expect(screen.getByTestId("turn-redo-keep").textContent).toBe("Continue from here");
  });

  it("keeps a player's own choice while the state stays, and starts over for the next state", () => {
    const { rerender } = render(ui(handoff));
    fireEvent.click(screen.getByTestId("turn-redo-close"));
    rerender(ui({ ...handoff }));
    expect(screen.getByTestId("turn-redo-bar")).toHaveAttribute("data-expanded", "false");
    rerender(ui(conflict));
    expect(screen.getByTestId("turn-redo-bar")).toHaveAttribute("data-expanded", "true");
  });

  it("does not open for a watcher, who has nothing to decide", () => {
    render(ui({ ...handoff, can_control: false }));
    expect(screen.getByTestId("turn-redo-bar")).toHaveAttribute("data-expanded", "false");
    expect(screen.queryByTestId("turn-redo-keep")).toBeNull();
  });

  it("opens to show a refusal", () => {
    render(ui(null, null, "Turn redo failed (403)"));
    expect(screen.getByTestId("turn-redo-error").textContent).toMatch(/403/);
  });

  it("clamps a long conflict text behind a Details button", () => {
    render(ui(conflict));
    const detail = screen.getByTestId("turn-redo-detail");
    expect(detail).toHaveClass("turn-redo__detail--clamped");
    const more = screen.getByTestId("turn-redo-more");
    expect(more).toHaveAttribute("aria-expanded", "false");
    expect(more.textContent).toBe("Details");
    fireEvent.click(more);
    expect(detail).not.toHaveClass("turn-redo__detail--clamped");
    expect(more).toHaveAttribute("aria-expanded", "true");
    expect(detail.textContent).toMatch(/hazardous exploration card Gamma Wormhole reserved for Ana is no longer in the deck/);
  });

  it("does not clamp a short text", () => {
    render(ui({ ...handoff, outcome: { kept: 3, tail_total: 3, stop: { kind: "tail_exhausted" }, asking_seat: null } }));
    expect(screen.getByTestId("turn-redo-detail").textContent).toMatch(/^All 3 recorded decisions/);
    expect(screen.queryByTestId("turn-redo-more")).toBeNull();
  });

  it("lives in the board's toolbar strip slot when there is one", () => {
    const slot = document.createElement("div");
    document.body.appendChild(slot);
    render(
      <BoardPrepSlotContext.Provider value={{ slot, setSlot: () => undefined }}>{ui(base)}</BoardPrepSlotContext.Provider>,
    );
    const bar = screen.getByTestId("turn-redo-bar");
    expect(slot.contains(bar)).toBe(true);
    expect(bar).not.toHaveClass("turn-redo--floating");
    slot.remove();
  });

  it("docks in the side column on a short landscape screen, even with a slot", () => {
    mockMatchMedia(true, true);
    const slot = document.createElement("div");
    document.body.appendChild(slot);
    render(
      <BoardPrepSlotContext.Provider value={{ slot, setSlot: () => undefined }}>{ui(base)}</BoardPrepSlotContext.Provider>,
    );
    const bar = screen.getByTestId("turn-redo-bar");
    expect(bar).toHaveClass("turn-redo--compact", "turn-redo--dock");
    expect(slot.contains(bar)).toBe(false);
    slot.remove();
  });

  it("floats without a slot", () => {
    render(ui(base));
    expect(screen.getByTestId("turn-redo-bar")).toHaveClass("turn-redo--floating");
  });

  it("follows a rotation back to the desktop strip", () => {
    const media = mockMatchMedia(true);
    render(ui(base));
    expect(screen.getByTestId("turn-redo-bar")).toHaveClass("turn-redo--compact");
    media.set(false);
    const bar = screen.getByTestId("turn-redo-bar");
    expect(bar).not.toHaveClass("turn-redo--compact");
    expect(bar).toHaveAttribute("role", "status");
    expect(screen.getByTestId("turn-redo-restore")).toBeVisible();
  });
});

describe("TurnRedoBar on a desktop", () => {
  beforeEach(() => {
    mockMatchMedia(false);
  });
  afterEach(() => {
    window.matchMedia = original;
  });

  it("keeps the full strip with its buttons in view", () => {
    render(ui(handoff));
    const bar = screen.getByTestId("turn-redo-bar");
    expect(bar).not.toHaveClass("turn-redo--compact");
    expect(screen.queryByTestId("turn-redo-toggle")).toBeNull();
    expect(screen.getByTestId("turn-redo-keep")).toBeVisible();
  });
});
