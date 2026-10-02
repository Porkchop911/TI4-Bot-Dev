import { useCallback, useEffect, useState } from "react";
import { PendingChoiceDto, PublicTurnStatus, ViewerRole } from "../protocol/types.ts";
import {
  ConnectionStatus,
  GameLogEntry,
  GameSessionClient,
  GameSessionState,
  serverEventLog,
  SnapshotState,
} from "../protocol/client.ts";

export { serverEventLog };
export type { ConnectionStatus, GameLogEntry, SnapshotState };

export interface UseGameSessionOptions {
  gameId: string;
  viewer: ViewerRole;
  serverUrl?: string;
}

export interface UseGameSessionReturn {
  status: ConnectionStatus;
  gameVersion: number;
  snapshot: SnapshotState | null;
  pendingChoice: PendingChoiceDto | null;
  turnStatus: PublicTurnStatus | null;
  lastError: string | null;
  events: GameLogEntry[];
  history: import("../protocol/types.ts").HistoryStatus;
  submitChoice: (optionId: string) => Promise<void>;
  changeHistory: (action: import("../protocol/client.ts").HistoryChange) => Promise<void>;
  submitMovementBatch: (
    destination: string,
    steps: import("../protocol/client.ts").MovementStep[],
  ) => Promise<void>;
  submitBatch: (plan: import("../protocol/client.ts").BasketPlan) => Promise<void>;
}

export function useGameSession({
  gameId,
  viewer,
  serverUrl,
}: UseGameSessionOptions): UseGameSessionReturn {
  const [client] = useState(() => new GameSessionClient({ gameId, viewer, serverUrl }));
  const [state, setState] = useState<GameSessionState>(() => client.getState());

  useEffect(() => {
    const unsubscribe = client.subscribe(() => setState(client.getState()));
    client.start();
    return () => {
      unsubscribe();
      client.stop();
    };
  }, [client]);

  const submitChoice = useCallback((optionId: string) => client.submitChoice(optionId), [client]);
  const changeHistory = useCallback(
    (action: import("../protocol/client.ts").HistoryChange) => client.changeHistory(action),
    [client],
  );

  return {
    ...state,
    submitChoice,
    changeHistory,
    submitMovementBatch: (destination, steps) => client.submitMovementBatch(destination, steps),
    submitBatch: (plan) => client.submitBatch(plan),
  };
}
