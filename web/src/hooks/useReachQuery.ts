import { useEffect, useState } from "react";
import type { ReachReply } from "../protocol/client.ts";
import { ReachUnsupportedError } from "../protocol/client.ts";
import type { ReachOrigin } from "../protocol/types.ts";

export type ReachState =
  | { status: "idle" | "loading" | "unsupported" | "error" }
  | { status: "ready"; systemId: string; origins: ReachOrigin[]; asOfVersion: number };

/**
 * Asks the server "who can go here" for `systemId` while `active`, again whenever `basis` (the
 * board the answer was about) changes. Stale answers for another system are never shown.
 */
export function useReachQuery(
  queryReach: ((systemId: string) => Promise<ReachReply>) | undefined,
  systemId: string | null,
  active: boolean,
  basis: unknown,
): ReachState {
  const [state, setState] = useState<ReachState>({ status: "idle" });
  useEffect(() => {
    if (!queryReach || !systemId || !active) {
      setState({ status: "idle" });
      return;
    }
    let cancelled = false;
    // Keep the previous answer on screen while a refresh for the same system is in flight.
    setState((prev) =>
      prev.status === "ready" && prev.systemId === systemId ? prev : { status: "loading" },
    );
    queryReach(systemId).then(
      (reply) => {
        if (cancelled) return;
        if (reply.kind === "reach") {
          setState({
            status: "ready",
            systemId: reply.systemId,
            origins: reply.origins,
            asOfVersion: reply.asOfVersion,
          });
        } else {
          setState({ status: "error" });
        }
      },
      (error: unknown) => {
        if (cancelled) return;
        setState({ status: error instanceof ReachUnsupportedError ? "unsupported" : "error" });
      },
    );
    return () => {
      cancelled = true;
    };
  }, [queryReach, systemId, active, basis]);
  return state;
}
