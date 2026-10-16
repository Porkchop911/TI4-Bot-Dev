import { useEffect } from "react";
import { onResume } from "../protocol/resilience.ts";

const devHeartbeatMs = Number(import.meta.env.VITE_TI4_DEV_PRESENCE_HEARTBEAT_MS);
/** A heartbeat unanswered after this long rode a dead connection. */
const HEARTBEAT_TIMEOUT_MS = 10_000;
// Also honoured in a production build (the smoke harness builds with it set); unset, it is 10 s.
const heartbeatMs = devHeartbeatMs > 0 ? devHeartbeatMs : 10_000;

/** Reports ephemeral presence; it does not renew the player session. */
export function usePresence(
  gameId: string,
  credential: string | undefined,
  onInvalid: () => void,
): void {
  useEffect(() => {
    if (!credential) return;
    let stopped = false;
    // A slow server must not stack one more heartbeat every tick (the browser runs out of sockets).
    let inFlight = false;
    let abort: (() => void) | null = null;
    let generation = 0;
    const renew = async (replaceInFlight = false) => {
      if (stopped) return;
      if (inFlight) {
        // Back from a long sleep the request in flight rides a dead connection: replace it.
        if (!replaceInFlight) return;
        abort?.();
      }
      inFlight = true;
      const mine = ++generation;
      const controller = new AbortController();
      const deadline = window.setTimeout(() => controller.abort(), HEARTBEAT_TIMEOUT_MS);
      abort = () => controller.abort();
      const response = await fetch(`/api/games/${encodeURIComponent(gameId)}/lobby/heartbeat`, {
        method: "POST",
        headers: { "x-ti4-player-session": credential },
        signal: controller.signal,
      }).catch(() => undefined);
      window.clearTimeout(deadline);
      // Only the newest request owns the flag (an aborted older one settles later).
      if (mine !== generation) return;
      inFlight = false;
      if (!stopped && response?.status === 403) onInvalid();
    };
    void renew();
    const timer = window.setInterval(() => void renew(), heartbeatMs);
    const stopResume = onResume(() => void renew(true));
    return () => {
      stopped = true;
      stopResume();
      window.clearInterval(timer);
    };
  }, [gameId, credential, onInvalid]);
}
