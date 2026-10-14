import { act, renderHook, waitFor } from "@testing-library/react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { HOST_FALLBACK_MS, useTurnRedo } from "./useTurnRedo.ts";
import type { TurnRedoCommand, TurnRedoStatus } from "../protocol/turnRedo.ts";

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

function setup(statuses: Array<TurnRedoStatus | null>, command = vi.fn().mockResolvedValue(undefined)) {
  let i = 0;
  const fetchStatus = vi.fn(async () => statuses[Math.min(i++, statuses.length - 1)]);
  const hook = renderHook(
    (props: { gameVersion: number; generation: number }) =>
      useTurnRedo({ enabled: true, fetchStatus, command, ...props }),
    { initialProps: { gameVersion: 5, generation: 0 } },
  );
  return { hook, fetchStatus, command };
}

describe("useTurnRedo", () => {
  it("reads the status on mount and does nothing for a spectator", async () => {
    const { hook } = setup([base]);
    await waitFor(() => expect(hook.result.current.status).toEqual(base));
    const fetchStatus = vi.fn();
    renderHook(() =>
      useTurnRedo({ enabled: false, fetchStatus, command: vi.fn(), gameVersion: 1, generation: 0 }),
    );
    expect(fetchStatus).not.toHaveBeenCalled();
  });

  it("asks for the round to replay once, as soon as the new turn is complete", async () => {
    const complete = { ...base, turn_complete: true };
    const { hook, command } = setup([base, complete]);
    await waitFor(() => expect(hook.result.current.status?.turn_complete).toBe(false));
    // The game moves on: the status is re-read, the turn is complete, autoplay fires.
    hook.rerender({ gameVersion: 6, generation: 0 });
    await waitFor(() => expect(command).toHaveBeenCalledWith({ action: "autoplay" }));
    expect(command).toHaveBeenCalledTimes(1);
  });

  it("does not ask again for an auto-play that went through when a stale status still says 'complete'", async () => {
    const complete = { ...base, turn_complete: true };
    // Every read (one started before the auto-play landed) still reports the new turn as complete.
    const { hook, command } = setup([complete]);
    await waitFor(() => expect(command).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(hook.result.current.busy).toBeNull());
    hook.rerender({ gameVersion: 6, generation: 0 });
    hook.rerender({ gameVersion: 7, generation: 0 });
    await new Promise((resolve) => setTimeout(resolve, 20));
    expect(command).toHaveBeenCalledTimes(1);
    expect(hook.result.current.error).toBeNull();
  });

  it("asks again for a later redo once the previous one is over", async () => {
    const complete = { ...base, turn_complete: true };
    const { hook, command, fetchStatus } = setup([complete]);
    await waitFor(() => expect(command).toHaveBeenCalledTimes(1));
    // The redo ends (status gone), then the same seat redoes the same turn again.
    fetchStatus.mockResolvedValue(null);
    hook.rerender({ gameVersion: 6, generation: 1 });
    await waitFor(() => expect(hook.result.current.status).toBeNull());
    fetchStatus.mockResolvedValue(complete);
    hook.rerender({ gameVersion: 7, generation: 2 });
    await waitFor(() => expect(command).toHaveBeenCalledTimes(2));
  });

  it("asks again after an auto-play the game outran (a quiet stale refusal)", async () => {
    const complete = { ...base, turn_complete: true };
    const command = vi
      .fn()
      .mockRejectedValueOnce(new Error("Game advanced or a decision is in flight: expected 5"))
      .mockResolvedValue(undefined);
    const { hook } = setup([complete], command);
    await waitFor(() => expect(command).toHaveBeenCalledTimes(1));
    await waitFor(() => expect(hook.result.current.busy).toBeNull());
    expect(hook.result.current.error).toBeNull();
    hook.rerender({ gameVersion: 6, generation: 0 });
    await waitFor(() => expect(command).toHaveBeenCalledTimes(2));
  });

  it("does not auto-play for a viewer who may not steer the redo", async () => {
    const { hook, command } = setup([{ ...base, turn_complete: true, can_control: false }]);
    await waitFor(() => expect(hook.result.current.status).not.toBeNull());
    expect(command).not.toHaveBeenCalled();
  });

  it("runs commands, reports the server's refusal and clears it on the next try", async () => {
    const command = vi
      .fn()
      .mockRejectedValueOnce(new Error("Turn redo failed (403): only the host may redo another seat's turn"))
      .mockResolvedValue(undefined);
    const onTimelineChanged = vi.fn();
    let i = 0;
    const fetchStatus = vi.fn(async () => (i++ === 0 ? null : base));
    const { result } = renderHook(() =>
      useTurnRedo({ enabled: true, fetchStatus, command, gameVersion: 1, generation: 0, onTimelineChanged }),
    );
    await act(async () => result.current.request({ turns: 2, seat: "p3" }));
    expect(command).toHaveBeenCalledWith({ action: "request", turns: 2, seat: "p3" });
    await waitFor(() => expect(result.current.error).toMatch(/only the host/));
    expect(onTimelineChanged).not.toHaveBeenCalled();
    await act(async () => result.current.request());
    await waitFor(() => expect(result.current.error).toBeNull());
    expect(command).toHaveBeenLastCalledWith({ action: "request", turns: 1, seat: undefined });
    expect(onTimelineChanged).toHaveBeenCalledOnce();
    await waitFor(() => expect(result.current.status).toEqual(base));
  });

  it("restores and keeps through the same command channel", async () => {
    const { hook, command } = setup([base]);
    await waitFor(() => expect(hook.result.current.status).not.toBeNull());
    await act(async () => hook.result.current.restore());
    await waitFor(() => expect(command).toHaveBeenCalledWith({ action: "restore" }));
    await act(async () => hook.result.current.keep());
    await waitFor(() => expect(command).toHaveBeenCalledWith({ action: "keep" }));
  });

  describe("one tab asks for the auto-play", () => {
    afterEach(() => vi.useRealTimers());
    const complete: TurnRedoStatus = { ...base, turn_complete: true };
    const renderFor = (viewerSeat: string, command: (c: TurnRedoCommand) => Promise<void>) =>
      renderHook(() =>
        useTurnRedo({
          enabled: true,
          fetchStatus: async () => complete,
          command,
          gameVersion: 5,
          generation: 0,
          viewerSeat,
        }),
      );

    it("the redoing seat's tab asks at once", async () => {
      const command = vi.fn().mockResolvedValue(undefined);
      renderFor("p2", command);
      await waitFor(() => expect(command).toHaveBeenCalledWith({ action: "autoplay" }));
    });

    it("the host's tab waits, then asks only if the redo is still waiting", async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const command = vi.fn().mockResolvedValue(undefined);
      const hook = renderFor("host", command);
      await vi.waitFor(() => expect(hook.result.current.status).not.toBeNull());
      await act(async () => {
        vi.advanceTimersByTime(HOST_FALLBACK_MS - 100);
      });
      expect(command).not.toHaveBeenCalled();
      await act(async () => {
        vi.advanceTimersByTime(200);
      });
      expect(command).toHaveBeenCalledTimes(1);
    });

    it("the host's tab stays quiet when the redo moved on before the delay ran out", async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const command = vi.fn().mockResolvedValue(undefined);
      let status: TurnRedoStatus = complete;
      const { result, rerender } = renderHook(
        (props: { gameVersion: number }) =>
          useTurnRedo({
            enabled: true,
            fetchStatus: async () => status,
            command,
            generation: 0,
            viewerSeat: "host",
            ...props,
          }),
        { initialProps: { gameVersion: 5 } },
      );
      await vi.waitFor(() => expect(result.current.status).not.toBeNull());
      status = { ...complete, stage: "auto_played" };
      rerender({ gameVersion: 6 });
      await vi.waitFor(() => expect(result.current.status?.stage).toBe("auto_played"));
      await act(async () => {
        vi.advanceTimersByTime(HOST_FALLBACK_MS + 500);
      });
      expect(command).not.toHaveBeenCalled();
    });
  });

  it("keeps the status object (no re-render) when a re-read returns the same status", async () => {
    const { hook } = setup([base, { ...base }]);
    await waitFor(() => expect(hook.result.current.status).toEqual(base));
    const first = hook.result.current.status;
    hook.rerender({ gameVersion: 6, generation: 0 });
    await waitFor(() => expect(hook.result.current.status).toEqual(base));
    expect(hook.result.current.status).toBe(first);
  });

  it("does not let an older status response overwrite a newer one", async () => {
    let resolveFirst!: (value: TurnRedoStatus | null) => void;
    const first = new Promise<TurnRedoStatus | null>((resolve) => (resolveFirst = resolve));
    const fetchStatus = vi
      .fn()
      .mockReturnValueOnce(first)
      .mockResolvedValue({ ...base, stage: "auto_played" });
    const hook = renderHook(
      (props: { generation: number }) =>
        useTurnRedo({ enabled: true, fetchStatus, command: vi.fn(), gameVersion: 5, ...props }),
      { initialProps: { generation: 0 } },
    );
    hook.rerender({ generation: 1 });
    await waitFor(() => expect(hook.result.current.status?.stage).toBe("auto_played"));
    await act(async () => resolveFirst(base));
    expect(hook.result.current.status?.stage).toBe("auto_played");
  });
});
