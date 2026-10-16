import { useCallback, useEffect, useMemo, useState } from "react";
import {
  normalizePlan,
  parseStoredPlan,
  serializeStoredPlan,
  type SecondaryPlan,
  type StoredPlan,
} from "../presentation/secondaryPlan.ts";

export const PREPARED_KEY_PREFIX = "ti4_secondary_prepared";
const key = (gameId: string, seat: string) => `${PREPARED_KEY_PREFIX}:${gameId}:${seat}`;

/** Reads the viewer's stored plan for this game; never throws. */
export function loadPlan(gameId: string, seat: string): StoredPlan | null {
  try {
    return parseStoredPlan(localStorage.getItem(key(gameId, seat)));
  } catch {
    return null;
  }
}

function savePlan(gameId: string, seat: string, stored: StoredPlan | null) {
  try {
    if (stored) localStorage.setItem(key(gameId, seat), serializeStoredPlan(stored));
    else localStorage.removeItem(key(gameId, seat));
  } catch {
    // No storage: the plan then lives in memory only.
  }
}

export interface UsePreparedPlanInput {
  gameId: string | undefined;
  viewerSeat: string | null | undefined;
  /** Identity of the strategic action in progress, or `null` when there is none. */
  actionKey: string | null;
  generation: number;
  /** The log and view are loaded; only then may a missing action count as "it ended". */
  ready: boolean;
}

/**
 * The viewer's prepared secondary for the action in progress. It lives on this device only (never
 * sent anywhere) and is valid for exactly one action: a different action (a new strategic action) or
 * none in progress (finished, cancelled, or undone past its start) drops it. `clear()` is the
 * player's own revoke.
 *
 * The history generation is deliberately NOT part of validity. Every committed batch (a movement,
 * a production or a token purchase by any seat) replaces the live session and bumps the generation,
 * so a plan tied to it was dropped in the middle of the very action it was made for. An undo, redo or
 * restore that matters changes what the log says: the action then disappears or is a different one,
 * and the key no longer matches. The generation is still stored, for diagnostics.
 */
export function usePreparedPlan({
  gameId,
  viewerSeat,
  actionKey,
  generation,
  ready,
}: UsePreparedPlanInput) {
  const [stored, setStored] = useState<StoredPlan | null>(() =>
    gameId && viewerSeat ? loadPlan(gameId, viewerSeat) : null,
  );

  // Another game or seat: read what that one stored.
  const identity = `${gameId ?? ""}:${viewerSeat ?? ""}`;
  const [seenIdentity, setSeenIdentity] = useState(identity);
  if (seenIdentity !== identity) {
    setSeenIdentity(identity);
    setStored(gameId && viewerSeat ? loadPlan(gameId, viewerSeat) : null);
  }

  const valid =
    stored !== null &&
    actionKey !== null &&
    stored.actionKey === actionKey;

  useEffect(() => {
    if (!stored || valid || !ready) return;
    // A different action (or the action is over): it can never apply again.
    setStored(null);
    if (gameId && viewerSeat) savePlan(gameId, viewerSeat, null);
  }, [stored, valid, ready, gameId, viewerSeat]);

  const set = useCallback(
    (plan: SecondaryPlan) => {
      if (!gameId || !viewerSeat || !actionKey) return;
      const next: StoredPlan = { v: 1, actionKey, generation, plan: normalizePlan(plan) };
      setStored(next);
      savePlan(gameId, viewerSeat, next);
    },
    [gameId, viewerSeat, actionKey, generation],
  );

  const clear = useCallback(() => {
    setStored(null);
    if (gameId && viewerSeat) savePlan(gameId, viewerSeat, null);
  }, [gameId, viewerSeat]);

  const plan = useMemo(() => (valid ? stored.plan : null), [valid, stored]);
  return { plan, set, clear };
}
