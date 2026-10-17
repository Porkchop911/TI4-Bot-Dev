import { act, fireEvent, render, screen } from "@testing-library/react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import type { GameEvent, LobbyDto } from "../protocol/types.ts";
import { PlayerIdentityProvider } from "../presentation/PlayerIdentity.tsx";
import type { ToastHistoryEntry } from "../presentation/toastHistory.ts";
import { CornerToastLayer } from "./CornerToastLayer.tsx";
import { NotificationLogButton, NotificationLogProvider, useNotificationLog } from "./NotificationLog.tsx";

const lobby = {
  slots: [
    { occupant: "p1", nickname: "Alex", position: 1 },
    { occupant: "p2", nickname: "Blair", position: 2 },
  ],
} as unknown as LobbyDto;

const entries: ToastHistoryEntry[] = [
  { id: "a", kind: "action", actor: "p2", text: "played Sabotage" },
  { id: "b", kind: "agenda", tag: "Agenda", text: "Mutiny passed" },
  { id: "c", kind: "action", actor: "p2", text: "activated system #22" },
];

/** Feeds the provider its entries the way the corner toast layer does. */
const Feed: React.FC<{ entries: readonly ToastHistoryEntry[] }> = ({ entries }) => {
  const { setEntries } = useNotificationLog();
  const feed = () => setEntries(entries);
  return <button onClick={feed}>feed</button>;
};

function page(inline: boolean, feed: readonly ToastHistoryEntry[] = entries) {
  return (
    <PlayerIdentityProvider lobby={lobby} seatingOrder={["p1", "p2"]}>
      <NotificationLogProvider inline={inline}>
        <div className="game-header__actions">
          <button data-testid="objectives-modal-button">Objectives</button>
          <NotificationLogButton />
        </div>
        <Feed entries={feed} />
      </NotificationLogProvider>
    </PlayerIdentityProvider>
  );
}

describe("notification log (single player)", () => {
  it("renders nothing outside a single-player game", () => {
    render(page(false));
    expect(screen.queryByTestId("toast-history-toggle")).toBeNull();
  });

  it("is an icon button with an accessible name and no visible words", () => {
    render(page(true));
    const toggle = screen.getByTestId("toast-history-toggle");
    expect(toggle).toHaveAttribute("aria-label", "Notifications");
    expect(toggle).toHaveAttribute("aria-expanded", "false");
    expect(toggle.textContent).toBe("");
    expect(toggle.querySelector("svg")).not.toBeNull();
    // Same row, same style as its neighbour.
    expect(toggle.parentElement).toBe(screen.getByTestId("objectives-modal-button").parentElement);
    expect(toggle).toHaveClass("button", "button--secondary");
  });

  it("counts what arrived since it was last open, and the list reads newest first", () => {
    render(page(true));
    fireEvent.click(screen.getByText("feed"));
    const toggle = screen.getByTestId("toast-history-toggle");
    expect(toggle).toHaveAttribute("aria-label", "Notifications, 3 new");
    expect(screen.getByTestId("toast-history-unread")).toHaveTextContent("3");
    fireEvent.click(toggle);
    expect(toggle).toHaveAttribute("aria-expanded", "true");
    const items = screen.getAllByTestId("toast-history-item").map((i) => i.textContent);
    expect(items).toEqual([
      expect.stringContaining("activated system #22"),
      expect.stringContaining("Mutiny passed"),
      expect.stringContaining("played Sabotage"),
    ]);
    // Everything shown counts as read.
    expect(screen.queryByTestId("toast-history-unread")).toBeNull();
    expect(toggle).toHaveAttribute("aria-label", "Notifications");
  });

  it("says so when there is nothing yet, and closes with Escape, the button or a click elsewhere", () => {
    render(page(true, []));
    const toggle = screen.getByTestId("toast-history-toggle");
    fireEvent.click(toggle);
    expect(screen.getByTestId("toast-history-empty")).toBeInTheDocument();
    fireEvent.keyDown(window, { key: "Escape" });
    expect(screen.queryByTestId("toast-history-panel")).toBeNull();
    fireEvent.click(toggle);
    fireEvent.click(toggle);
    expect(screen.queryByTestId("toast-history-panel")).toBeNull();
    fireEvent.click(toggle);
    fireEvent.pointerDown(document.body);
    expect(screen.queryByTestId("toast-history-panel")).toBeNull();
  });
});

describe("corner toasts and the notification log", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  const ev = (id: string, detail: string): GameEvent =>
    ({
      id,
      timestamp: "1",
      visibility: { visibility: "public" as const },
      event: { kind: "decision_resolved" },
      actor: "p2",
      detail: `p2 ${detail}`,
    }) as GameEvent;

  const layer = (inline: boolean, events: GameEvent[]) => (
    <PlayerIdentityProvider lobby={lobby} seatingOrder={["p1", "p2"]}>
      <NotificationLogProvider inline={inline}>
        <NotificationLogButton />
        <CornerToastLayer events={events} viewerSeat="p1" ready />
      </NotificationLogProvider>
    </PlayerIdentityProvider>
  );

  it("keeps a swiped-away toast in the log, and puts no pill under the toasts", () => {
    const { rerender } = render(layer(true, []));
    expect(screen.queryByText(/Recent notifications/)).toBeNull();
    rerender(layer(true, [ev("1", "played Sabotage")]));
    expect(screen.getAllByTestId("corner-toast")).toHaveLength(1);
    const toast = screen.getByTestId("corner-toast");
    fireEvent.pointerDown(toast, { pointerId: 1, clientX: 200, clientY: 10, pointerType: "touch" });
    fireEvent.pointerMove(toast, { pointerId: 1, clientX: 120, clientY: 10, pointerType: "touch" });
    act(() => vi.advanceTimersByTime(40));
    fireEvent.pointerUp(toast, { pointerId: 1, clientX: 120, clientY: 10, pointerType: "touch" });
    act(() => vi.advanceTimersByTime(400));
    expect(screen.queryByTestId("corner-toast")).toBeNull();
    fireEvent.click(screen.getByTestId("toast-history-toggle"));
    expect(screen.getByTestId("toast-history-item")).toHaveTextContent("played Sabotage");
    // The toast column sits under the controls in a single-player game.
    expect(screen.getByTestId("corner-toasts")).toHaveClass("auto-resolve-toast-container--under");
  });

  it("keeps the corner pill only where there is no provider", () => {
    const bare = (events: GameEvent[]) => (
      <PlayerIdentityProvider lobby={lobby} seatingOrder={["p1", "p2"]}>
        <CornerToastLayer events={events} viewerSeat="p1" ready />
      </PlayerIdentityProvider>
    );
    const { rerender } = render(bare([]));
    rerender(bare([ev("1", "played Sabotage")]));
    expect(screen.getByText(/Recent notifications \(1\)/)).toBeInTheDocument();
    expect(screen.getByTestId("corner-toasts")).not.toHaveClass("auto-resolve-toast-container--under");
  });

  describe("in a multiplayer game", () => {
    const from = (id: string, actor: string, detail: string): GameEvent => ({ ...ev(id, detail), actor, detail: `${actor} ${detail}` }) as GameEvent;
    const mp = (events: GameEvent[], viewerSeat: string | undefined) => (
      <PlayerIdentityProvider lobby={lobby} seatingOrder={["p1", "p2"]}>
        <NotificationLogProvider inline>
          <NotificationLogButton />
          <CornerToastLayer events={events} viewerSeat={viewerSeat} ready />
        </NotificationLogProvider>
      </PlayerIdentityProvider>
    );

    it("lists the actions newest first with the actor's label and counts them unread (spectator sees every seat)", () => {
      const { rerender } = render(mp([], undefined));
      rerender(mp([from("1", "p2", "played Sabotage")], undefined));
      rerender(mp([from("1", "p2", "played Sabotage"), from("2", "p1", "played Mutiny")], undefined));
      expect(screen.queryByText(/Recent notifications \(/)).toBeNull();
      expect(screen.getByTestId("toast-history-unread")).toHaveTextContent("2");
      fireEvent.click(screen.getByTestId("toast-history-toggle"));
      const items = screen.getAllByTestId("toast-history-item");
      expect(items).toHaveLength(2);
      expect(items[0]).toHaveTextContent("played Mutiny");
      expect(items[1]).toHaveTextContent("played Sabotage");
      expect(items[0].querySelector(".toast-history__actor")).not.toBeNull();
      expect(screen.queryByTestId("toast-history-unread")).toBeNull();
    });

    it("a seat's log holds other seats' actions only", () => {
      const { rerender } = render(mp([], "p1"));
      rerender(mp([from("1", "p1", "played Sabotage"), from("2", "p2", "played Mutiny")], "p1"));
      fireEvent.click(screen.getByTestId("toast-history-toggle"));
      const items = screen.getAllByTestId("toast-history-item");
      expect(items).toHaveLength(1);
      expect(items[0]).toHaveTextContent("played Mutiny");
    });

    it("does not repeat entries when a resync delivers the same log again, nor on a reload", () => {
      const log = [from("1", "p2", "played Sabotage")];
      const { rerender } = render(mp([], "p1"));
      rerender(mp(log, "p1"));
      rerender(mp([...log], "p1"));
      rerender(mp(log.map((e) => ({ ...e })), "p1"));
      fireEvent.click(screen.getByTestId("toast-history-toggle"));
      expect(screen.getAllByTestId("toast-history-item")).toHaveLength(1);
    });
  });
});
