import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import type { GameEvent, LobbyDto } from "../protocol/types.ts";
import { CornerToastLayer } from "./CornerToastLayer.tsx";
import { PlayerIdentityProvider } from "../presentation/PlayerIdentity.tsx";
import { TOAST_DURATION_MS } from "./AutoResolveToast.tsx";

const lobby = {
  slots: [
    { occupant: "p1", nickname: "Alex", position: 1 },
    { occupant: "p2", nickname: "Blair", position: 2 },
  ],
} as unknown as LobbyDto;

const ev = (id: string, actor: string, detail: string): GameEvent =>
  ({
    id,
    timestamp: "1",
    visibility: { visibility: "public" as const },
    event: { kind: "decision_resolved" },
    actor,
    detail: `${actor} ${detail}`,
  }) as GameEvent;

function layer(events: GameEvent[], viewerSeat: string | undefined = "p1") {
  return (
    <PlayerIdentityProvider lobby={lobby} seatingOrder={["p1", "p2"]}>
      <CornerToastLayer events={events} viewerSeat={viewerSeat} ready />
    </PlayerIdentityProvider>
  );
}

describe("CornerToastLayer", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  it("is an always-present polite live region", () => {
    render(layer([]));
    const region = screen.getByTestId("corner-toasts");
    expect(region).toHaveAttribute("aria-live", "polite");
    expect(region).toHaveAttribute("role", "log");
  });

  it("shows another player's action with their name and seat colour, never your own", () => {
    const { rerender } = render(layer([]));
    rerender(layer([ev("1", "p2", "played Sabotage"), ev("2", "p1", "played Direct Hit")]));
    const toasts = screen.getAllByTestId("corner-toast");
    expect(toasts).toHaveLength(1);
    expect(toasts[0]).toHaveTextContent("Blair played Sabotage");
    expect(screen.getByTestId("toast-actor")).toHaveTextContent("Blair");
    expect(toasts[0].style.getPropertyValue("--toast-accent")).toBe("#4ADE80");
  });

  it("hides on its own after a few seconds", () => {
    const { rerender } = render(layer([]));
    rerender(layer([ev("1", "p2", "activated #22")]));
    expect(screen.getByText(/activated system #22/)).toBeInTheDocument();
    act(() => vi.advanceTimersByTime(TOAST_DURATION_MS.action + 10));
    expect(screen.getByTestId("corner-toast")).toHaveClass("dismissing");
    act(() => vi.advanceTimersByTime(400));
    expect(screen.queryByTestId("corner-toast")).not.toBeInTheDocument();
  });

  it("dismisses on click", () => {
    const { rerender } = render(layer([]));
    rerender(layer([ev("1", "p2", "activated #22")]));
    fireEvent.click(screen.getByTestId("corner-toast"));
    act(() => vi.advanceTimersByTime(400));
    expect(screen.queryByTestId("corner-toast")).not.toBeInTheDocument();
  });

  it("is lifted above a turn action bar that is in the way", () => {
    const bar = document.createElement("div");
    bar.className = "turn-bar";
    bar.getBoundingClientRect = () => ({ top: window.innerHeight - 120, left: 0, height: 120 }) as DOMRect;
    document.body.appendChild(bar);
    render(layer([]));
    expect(screen.getByTestId("corner-toasts").style.bottom).toBe("132px");
    bar.remove();
  });
});
