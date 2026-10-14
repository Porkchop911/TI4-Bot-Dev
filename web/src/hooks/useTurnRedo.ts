import { useCallback, useEffect, useRef, useState } from "react";
import type { TurnRedoCommand, TurnRedoStatus } from "../protocol/turnRedo.ts";

export type TurnRedoBusy = "request" | "autoplay" | "restore" | "keep" | null;

export interface UseTurnRedoOptions {
  /** Seated players only: a spectator has no credential to read the status with. */
  enabled: boolean;
  fetchStatus: () => Promise<TurnRedoStatus | null>;
  command: (command: TurnRedoCommand) => Promise<void>;
  /** The live game version: a redo in flight is re-read as the game moves. */
  gameVersion: number;
  /** Bumps whenever the timeline is replaced, by anyone. */
  generation: number;
  /** After the timeline was replaced by this client's own command. */
  onTimelineChanged?: () => void;
  /**
   * The seat this client plays. Only the redoing seat's tab asks for the auto-play at once; any
   * other controlling tab (the host's) waits `HOST_FALLBACK_MS` and asks only if the redo is still
   * waiting. Left out, every controlling tab asks at once.
   */
  viewerSeat?: string | null;
}

/** How long the host's tab waits for the redoing seat's tab before it asks for the auto-play itself. */
export const HOST_FALLBACK_MS = 6000;

export interface UseTurnRedoResult {
  status: TurnRedoStatus | null;
  busy: TurnRedoBusy;
  error: string | null;
  request: (options?: { turns?: 1 | 2; seat?: string }) => void;
  autoplay: () => void;
  restore: () => void;
  keep: () => void;
}

/** One redo of a chain: a new request (or a second redo of the chain) is a new key. */
const redoKey = (status: TurnRedoStatus) =>
  `${status.seat}:${status.rewound_to}:${status.redo_count}:${status.original_decisions}`;

/** A refusal that only means the game moved on while the request was in flight: try again later. */
const isStale = (error: unknown) =>
  error instanceof Error && /Game advanced|decision is in flight|\(409\)/.test(error.message);

/**
 * Turn redo state for one client: reads the server's status, runs the commands, and fires the
 * auto-play by itself as soon as the redoing seat's new turn is complete (the server never moves
 * on its own, so something has to ask for the round to replay).
 */
export function useTurnRedo({
  enabled,
  fetchStatus,
  command,
  gameVersion,
  generation,
  onTimelineChanged,
  viewerSeat,
}: UseTurnRedoOptions): UseTurnRedoResult {
  const [status, setStatus] = useState<TurnRedoStatus | null>(null);
  const [busy, setBusy] = useState<TurnRedoBusy>(null);
  const [error, setError] = useState<string | null>(null);
  const autoplayedAt = useRef<number | null>(null);
  const busyRef = useRef<TurnRedoBusy>(null);
  busyRef.current = busy;

  const latestRead = useRef(0);
  const refresh = useCallback(async () => {
    if (!enabled) return;
    const read = ++latestRead.current;
    try {
      const next = await fetchStatus();
      // An older response must not overwrite a newer one, and an unchanged status keeps its
      // identity, so the bar does not re-render (and its buttons do not flicker) for nothing.
      if (read !== latestRead.current) return;
      setStatus((previous) =>
        JSON.stringify(previous) === JSON.stringify(next) ? previous : next,
      );
    } catch {
      // A status that cannot be read (an older server, a dropped request) is not a redo that is
      // not happening, and not worth a banner of its own: keep what we know and try again on the
      // next change.
    }
  }, [enabled, fetchStatus]);

  // On mount and whenever the timeline is replaced.
  useEffect(() => {
    void refresh();
  }, [refresh, generation]);

  // While a redo is in flight, follow the game: the new turn completes with some decision.
  const inFlight = useRef(false);
  inFlight.current = status !== null;
  useEffect(() => {
    if (inFlight.current) void refresh();
  }, [refresh, gameVersion]);

  // The redo whose auto-play already went through. A status read that started before it landed can
  // still say "new turn, complete" at a newer game version; asking again then only made the server
  // answer a no-op (and, before the client knew that, reconnect every socket for nothing).
  const autoplayedRedo = useRef<string | null>(null);
  useEffect(() => {
    if (status === null) autoplayedRedo.current = null;
  }, [status]);

  const run = useCallback(
    async (kind: Exclude<TurnRedoBusy, null>, cmd: TurnRedoCommand, quiet = false): Promise<boolean> => {
      if (busyRef.current) return false;
      setBusy(kind);
      busyRef.current = kind;
      setError(null);
      try {
        await command(cmd);
        // A new redo (or the original back) starts over: its auto-play is still to come.
        if (kind === "request" || kind === "restore") autoplayedRedo.current = null;
        onTimelineChanged?.();
        await refresh();
        return true;
      } catch (cause) {
        if (!(quiet && isStale(cause)))
          setError(cause instanceof Error ? cause.message : String(cause));
        return false;
      } finally {
        busyRef.current = null;
        setBusy(null);
      }
    },
    [command, onTimelineChanged, refresh],
  );

  const autoplay = useCallback(() => void run("autoplay", { action: "autoplay" }), [run]);

  // The new turn is complete: ask the server to replay the round, once per game version. The
  // redoing seat's tab asks at once; the host's tab only as a fallback after a delay, so the two do
  // not race (the loser used to get a 409 or a 400 on every redo). The server also treats a repeat
  // as a no-op, so a race that still happens is harmless.
  const gameVersionRef = useRef(gameVersion);
  gameVersionRef.current = gameVersion;
  const statusRef = useRef(status);
  statusRef.current = status;
  const waiting =
    status?.stage === "new_turn" && status.turn_complete && status.can_control ? status.seat : null;
  const mine = waiting !== null && (viewerSeat === undefined || viewerSeat === waiting);
  useEffect(() => {
    if (waiting === null) return;
    const ask = () => {
      const current = statusRef.current;
      if (
        current?.stage !== "new_turn" ||
        !current.turn_complete ||
        busyRef.current ||
        autoplayedAt.current === gameVersionRef.current
      )
        return;
      const redo = redoKey(current);
      if (autoplayedRedo.current === redo) return;
      autoplayedAt.current = gameVersionRef.current;
      void run("autoplay", { action: "autoplay" }, true).then((done) => {
        if (done) autoplayedRedo.current = redo;
      });
    };
    if (mine) {
      ask();
      return;
    }
    const timer = setTimeout(ask, HOST_FALLBACK_MS);
    return () => clearTimeout(timer);
  }, [waiting, mine, gameVersion, run]);

  return {
    status,
    busy,
    error,
    request: (options) =>
      void run("request", { action: "request", turns: options?.turns ?? 1, seat: options?.seat }),
    autoplay,
    restore: () => void run("restore", { action: "restore" }),
    keep: () => void run("keep", { action: "keep" }),
  };
}
