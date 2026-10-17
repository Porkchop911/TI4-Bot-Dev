import { useCallback, useEffect, useMemo, useState } from "react";
import { PendingChoiceDto, PublicTurnStatus, ViewerRole } from "../protocol/types.ts";
import {
  ConnectionStatus,
  GameLogEntry,
  GameSessionClient,
  GameSessionState,
  serverEventLog,
  SnapshotState,
} from "../protocol/client.ts";
import type { GameTransport, TransportCapabilities } from "../protocol/transport.ts";

export { serverEventLog };
export type { ConnectionStatus, GameLogEntry, SnapshotState };

export interface UseGameSessionOptions {
  gameId: string;
  viewer: ViewerRole;
  serverUrl?: string;
  /**
   * Builds the transport for this session (default: the server WebSocket/HTTP client). The
   * offline mode passes its local transport here.
   */
  createTransport?: () => GameTransport;
}

export interface UseGameSessionReturn {
  /** Which optional features this session's transport offers; hide the rest. */
  capabilities: TransportCapabilities;
  status: ConnectionStatus;
  gameVersion: number;
  snapshot: SnapshotState | null;
  pendingChoice: PendingChoiceDto | null;
  turnStatus: PublicTurnStatus | null;
  lastError: string | null;
  /** Automatic reconnecting gave up, or the game is gone: show `message` with a Retry. */
  fatal: { kind: "gone" | "unreachable"; message: string } | null;
  /** The connection was lost and the server has not answered on a new one yet. */
  reconnecting?: boolean;
  /** Try to reconnect now (the Retry button). */
  retryConnection: () => void;
  events: GameLogEntry[];
  history: import("../protocol/types.ts").HistoryStatus;
  batchResume?: import("../protocol/client.ts").BatchResume | null;
  submitChoice: (optionId: string, nonce?: string) => Promise<void>;
  /** This seat's bluff settings as the server holds them (null until it has answered). */
  reactionIntent: import("../protocol/types.ts").ReactionIntentStateMsg | null;
  /** Declare the reaction windows this seat bluffs about (whole set; empty clears). */
  setReactionIntent: (triggers: string[]) => void;
  /** End the running bluff hold early. */
  passReactionHold: () => void;
  /** Never (or again) offer one action card, by printed name, to this seat. */
  setReactionMode: (card: string, mode: import("../protocol/types.ts").ReactionModeSetting) => void;
  changeHistory: (action: import("../protocol/client.ts").HistoryChange) => Promise<void>;
  /** The game's replay JSON for copying out (any seated player). */
  fetchReplay: () => Promise<{ text: string; filename: string }>;
  /** Turn redo: where it stands (null when none is in flight), and the commands that change it. */
  fetchTurnRedoStatus: () => Promise<import("../protocol/turnRedo.ts").TurnRedoStatus | null>;
  turnRedoCommand: (command: import("../protocol/turnRedo.ts").TurnRedoCommand) => Promise<void>;
  submitMovementBatch: (
    destination: string,
    steps: import("../protocol/client.ts").MovementStep[],
  ) => Promise<void>;
  submitBatch: (plan: import("../protocol/client.ts").BasketPlan) => Promise<void>;
  /** Read-only: what this seat would be asked for the secondary in progress, as of now. */
  previewSecondary: (
    card: string,
    primary: string,
    answers: readonly string[],
  ) => Promise<import("../protocol/client.ts").SecondaryPreviewReply>;
  /** Read-only "who can go here": the ships that could move into a system if it were activated now. */
  queryReach: (systemId: string) => Promise<import("../protocol/client.ts").ReachReply>;
  /** The engine of a game that runs on this device (`GameTransport.queryEngine`); absent online. */
  queryEngine?: (request: object) => Promise<unknown>;
  resumeBatch: () => Promise<void>;
  dismissBatchResume: () => void;
}

export function useGameSession({
  gameId,
  viewer,
  serverUrl,
  createTransport,
}: UseGameSessionOptions): UseGameSessionReturn {
  const [client] = useState<GameTransport>(() =>
    createTransport ? createTransport() : new GameSessionClient({ gameId, viewer, serverUrl }),
  );
  const [state, setState] = useState<GameSessionState>(() => client.getState());

  useEffect(() => {
    const unsubscribe = client.subscribe(() => setState(client.getState()));
    client.start();
    return () => {
      unsubscribe();
      client.stop();
    };
  }, [client]);

  const submitChoice = useCallback(
    (optionId: string, nonce?: string) => client.submitChoice(optionId, nonce),
    [client],
  );
  const changeHistory = useCallback(
    (action: import("../protocol/client.ts").HistoryChange) => client.changeHistory(action),
    [client],
  );

  // Stable identity: the who-can-go-here hook re-asks whenever it changes.
  const queryReach = useCallback((systemId: string) => client.queryReach(systemId), [client]);
  // Stable identity: the odds engine of the page keys on it.
  const queryEngine = useMemo(() => client.queryEngine?.bind(client), [client]);

  // Stable identities: the turn redo hook re-reads its status whenever these change, so a new
  // function per render made every status response trigger the next request (and a re-render).
  const fetchTurnRedoStatus = useCallback(() => client.fetchTurnRedoStatus(), [client]);
  const turnRedoCommand = useCallback(
    (command: import("../protocol/turnRedo.ts").TurnRedoCommand) => client.turnRedoCommand(command),
    [client],
  );

  return {
    ...state,
    capabilities: client.capabilities,
    submitChoice,
    fatal: state.fatal ?? null,
    retryConnection: () => client.retryNow(),
    reactionIntent: state.reactionIntent ?? null,
    setReactionIntent: (triggers) => client.setReactionIntent(triggers),
    passReactionHold: () => client.passReactionHold(),
    setReactionMode: (card, mode) => client.setReactionMode(card, mode),
    changeHistory,
    fetchReplay: () => client.fetchReplay(),
    fetchTurnRedoStatus,
    turnRedoCommand,
    submitMovementBatch: (destination, steps) => client.submitMovementBatch(destination, steps),
    submitBatch: (plan) => client.submitBatch(plan),
    previewSecondary: (card, primary, answers) => client.previewSecondary(card, primary, answers),
    queryReach,
    queryEngine,
    resumeBatch: () => client.resumeBatch(),
    dismissBatchResume: () => client.dismissBatchResume(),
  };
}
