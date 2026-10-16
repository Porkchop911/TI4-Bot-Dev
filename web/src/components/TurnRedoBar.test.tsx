import { fireEvent, render, screen } from "@testing-library/react";
import { describe, expect, it, vi } from "vitest";
import { TurnRedoBar } from "./TurnRedoBar.tsx";
import type { TurnRedoStatus } from "../protocol/turnRedo.ts";
import { PlayerIdentityProvider } from "../presentation/PlayerIdentity.tsx";
import type { LobbyDto } from "../protocol/types.ts";

const lobby: LobbyDto = {
  game_id: "g",
  phase: "running",
  lobby_version: 1,
  host_player_id: "p1",
  slots: [
    { slot_id: "s1", position: 1, occupant: "p1", nickname: "Ana", connected: true, can_take_over: false },
    { slot_id: "s2", position: 2, occupant: "p2", nickname: "Bo", connected: true, can_take_over: false },
    { slot_id: "s3", position: 3, occupant: "p3", nickname: "Cy", connected: true, can_take_over: false },
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

const handlers = { onAutoplay: vi.fn(), onRestore: vi.fn(), onKeep: vi.fn() };
const renderBar = (status: TurnRedoStatus | null, busy = null as null | "autoplay", error = null as string | null) =>
  render(
    <PlayerIdentityProvider lobby={lobby} seatingOrder={["p1", "p2", "p3"]}>
      <TurnRedoBar status={status} busy={busy} error={error} {...handlers} />
    </PlayerIdentityProvider>,
  );

describe("TurnRedoBar", () => {
  it("renders nothing when no redo is in flight", () => {
    const { container } = renderBar(null);
    expect(container).toBeEmptyDOMElement();
  });

  it("waits for the new turn and offers the way back", () => {
    renderBar(base);
    const bar = screen.getByTestId("turn-redo-bar");
    expect(bar).toHaveAttribute("data-state", "new-turn");
    expect(bar).toHaveAttribute("role", "status");
    expect(bar.textContent).toMatch(/Redoing Bo's last turn/);
    expect(screen.queryByTestId("turn-redo-keep")).toBeNull();
    fireEvent.click(screen.getByTestId("turn-redo-restore"));
    expect(handlers.onRestore).toHaveBeenCalledOnce();
  });

  it("names two turns", () => {
    renderBar({ ...base, turns_back: 2 });
    expect(screen.getByTestId("turn-redo-bar").textContent).toMatch(/last two turns/);
  });

  it("shows a status while the round replays, busy with no buttons", () => {
    renderBar({ ...base, turn_complete: true }, "autoplay");
    const bar = screen.getByTestId("turn-redo-bar");
    expect(bar).toHaveAttribute("data-state", "replaying");
    expect(bar).toHaveAttribute("aria-busy", "true");
    expect(bar.textContent).toMatch(/Replaying the round/);
    expect(screen.queryByTestId("turn-redo-replay")).toBeNull();
    // Restore stays (disabled) instead of leaving the bar while the request runs.
    expect(screen.getByTestId("turn-redo-restore")).toBeDisabled();
  });

  it("keeps the very same Restore element while a replay starts and ends (no remount, no flicker)", () => {
    const status = { ...base, turn_complete: true };
    const view = renderBar(status);
    const restore = screen.getByTestId("turn-redo-restore");
    view.rerender(
      <PlayerIdentityProvider lobby={lobby} seatingOrder={["p1", "p2", "p3"]}>
        <TurnRedoBar status={status} busy="autoplay" error={null} {...handlers} />
      </PlayerIdentityProvider>,
    );
    expect(screen.getByTestId("turn-redo-restore")).toBe(restore);
    view.rerender(
      <PlayerIdentityProvider lobby={lobby} seatingOrder={["p1", "p2", "p3"]}>
        <TurnRedoBar status={{ ...status }} busy={null} error={null} {...handlers} />
      </PlayerIdentityProvider>,
    );
    expect(screen.getByTestId("turn-redo-restore")).toBe(restore);
  });

  it("offers a manual replay when the new turn is done and nothing is running", () => {
    renderBar({ ...base, turn_complete: true });
    fireEvent.click(screen.getByTestId("turn-redo-replay"));
    expect(handlers.onAutoplay).toHaveBeenCalledOnce();
  });

  it("reports a hand-off back to the redoing seat with keep and restore", () => {
    renderBar({
      ...base,
      stage: "auto_played",
      handoff_len: 112,
      outcome: {
        kept: 9,
        tail_total: 30,
        stop: { kind: "handoff", seat: "p2" },
        asking_seat: "p2",
      },
    });
    const bar = screen.getByTestId("turn-redo-bar");
    expect(bar).toHaveAttribute("data-state", "handoff");
    expect(bar.textContent).toMatch(/Back to Bo/);
    expect(bar.textContent).toMatch(/next turn begins/);
    expect(bar.textContent).toMatch(/9 recorded decisions were replayed and kept/);
    expect(bar.textContent).toMatch(/that seat's own reactions and votes/);
    fireEvent.click(screen.getByTestId("turn-redo-keep"));
    expect(handlers.onKeep).toHaveBeenCalledOnce();
    expect(screen.getByTestId("turn-redo-keep").textContent).toBe("Keep this timeline");
  });

  it("explains a conflict without naming the recorded choice and offers continue or restore", () => {
    renderBar({
      ...base,
      stage: "auto_played",
      handoff_len: 106,
      outcome: {
        kept: 1,
        tail_total: 9,
        stop: {
          kind: "conflict",
          conflict: {
            original_cursor: 108,
            kind: "reserved_card",
            seat: "p3",
            prompt: "action phase",
            detail: "…",
            deck: "secret",
            card: null,
            recipient: "p2",
          },
        },
        asking_seat: "p3",
      },
    });
    const bar = screen.getByTestId("turn-redo-bar");
    expect(bar).toHaveAttribute("data-state", "conflict");
    expect(bar.textContent).toMatch(/1 recorded decision was kept/);
    expect(bar.textContent).toMatch(/a secret objective reserved for Bo is no longer in the deck/);
    expect(bar.textContent).not.toMatch(/deck offset|different number of cards/);
    expect(bar.textContent).toMatch(/Cy decides/);
    expect(screen.getByTestId("turn-redo-keep").textContent).toBe("Continue from here");
    fireEvent.click(screen.getByTestId("turn-redo-restore"));
    expect(handlers.onRestore).toHaveBeenCalled();
  });

  it("says the replay stops at the next turn and names a public card that is gone", () => {
    renderBar(base);
    expect(screen.getByTestId("turn-redo-bar").textContent).toMatch(/stop when your next turn begins/);
    renderBar({
      ...base,
      stage: "auto_played",
      handoff_len: 106,
      outcome: {
        kept: 2,
        tail_total: 9,
        stop: {
          kind: "conflict",
          conflict: {
            original_cursor: 108,
            kind: "reserved_card",
            seat: "p3",
            prompt: "explore",
            detail: "…",
            deck: "exploration:hazardous",
            card: "gamma_wormhole",
            recipient: "p3",
          },
        },
        asking_seat: "p3",
      },
    });
    const bars = screen.getAllByTestId("turn-redo-bar");
    expect(bars[1].textContent).toMatch(/the hazardous exploration card .* reserved for Cy is no longer in the deck/);
  });

  it("reports an exhausted tail", () => {
    renderBar({
      ...base,
      stage: "auto_played",
      handoff_len: 120,
      outcome: { kept: 4, tail_total: 4, stop: { kind: "tail_exhausted" }, asking_seat: null },
    });
    expect(screen.getByTestId("turn-redo-bar")).toHaveAttribute("data-state", "complete");
  });

  it("says plainly that a turn redone mid-turn had nothing to replay", () => {
    renderBar({
      ...base,
      stage: "auto_played",
      handoff_len: 120,
      outcome: { kept: 0, tail_total: 0, stop: { kind: "tail_exhausted" }, asking_seat: null },
    });
    const bar = screen.getByTestId("turn-redo-bar");
    expect(bar).toHaveAttribute("data-state", "complete");
    expect(bar.textContent).toMatch(/new turn is done/);
    expect(bar.textContent).toMatch(/nothing to replay/);
    expect(bar.textContent).not.toMatch(/All 0 recorded decisions/);
    expect(screen.getByTestId("turn-redo-keep")).toBeVisible();
    expect(screen.getByTestId("turn-redo-restore")).toBeVisible();
  });

  it("lets other players watch but not steer", () => {
    renderBar({ ...base, can_control: false, stage: "auto_played", handoff_len: 1, outcome: {
      kept: 0, tail_total: 0, stop: { kind: "tail_exhausted" }, asking_seat: null } });
    expect(screen.getByTestId("turn-redo-bar")).toBeVisible();
    expect(screen.queryByTestId("turn-redo-restore")).toBeNull();
    expect(screen.queryByTestId("turn-redo-keep")).toBeNull();
  });

  it("shows the server's refusal", () => {
    renderBar(null, null, "Turn redo failed (403): only the host may redo another seat's turn");
    expect(screen.getByTestId("turn-redo-error").textContent).toMatch(/only the host/);
  });

  it("lets a watching player hide the bar, and shows it again when the redo moves on", () => {
    const watching: TurnRedoStatus = { ...base, can_control: false };
    const view = (status: TurnRedoStatus) => (
      <PlayerIdentityProvider lobby={lobby} seatingOrder={["p1", "p2", "p3"]}>
        <TurnRedoBar status={status} busy={null} error={null} {...handlers} />
      </PlayerIdentityProvider>
    );
    const { rerender } = render(view(watching));
    fireEvent.click(screen.getByTestId("turn-redo-hide"));
    expect(screen.queryByTestId("turn-redo-bar")).toBeNull();
    rerender(view({ ...watching, turn_complete: true }));
    expect(screen.getByTestId("turn-redo-bar")).toHaveAttribute("data-state", "replaying");
  });

  it("does not offer Hide where Restore or Keep is the answer", () => {
    renderBar(base);
    expect(screen.queryByTestId("turn-redo-hide")).toBeNull();
  });
});
