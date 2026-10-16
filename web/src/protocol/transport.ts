import type {
  BatchPlan,
  GameSessionState,
  HistoryChange,
  MovementStep,
  ReachReply,
  SecondaryPreviewReply,
} from "./client.ts";
import { reduceServerMessage } from "./client.ts";
import type { ReactionModeSetting, ServerMessage } from "./types.ts";
import type { SpliceEdit, SplicePreview } from "./splice.ts";
import type { TurnRedoCommand, TurnRedoStatus } from "./turnRedo.ts";

import type { TransportCapabilities } from "./capabilities.ts";
export { SERVER_CAPABILITIES } from "./capabilities.ts";
export type { TransportCapabilities };

type Listener = () => void;

/**
 * One game session as `useGameSession` consumes it: an observable {@link GameSessionState}
 * plus the commands the UI can issue. A transport turns the commands into whatever its backend
 * needs, and its state evolves only by feeding {@link ServerMessage} objects through
 * `reduceServerMessage` (see {@link MessageDrivenState}), so every transport shows the same
 * state for the same messages.
 *
 * Methods whose capability flag is false may reject; the UI must not call them.
 */
export interface GameTransport {
  readonly capabilities: TransportCapabilities;

  getState(): GameSessionState;
  /** The listener runs after every state change; returns the unsubscribe function. */
  subscribe(listener: Listener): () => void;
  start(): void;
  stop(): void;
  /** Try to reconnect now (the Retry button). */
  retryNow(): void;

  submitChoice(optionId: string): Promise<void>;
  submitBatch(plan: BatchPlan): Promise<void>;
  submitMovementBatch(destination: string, steps: MovementStep[]): Promise<void>;
  resumeBatch(): Promise<void>;
  dismissBatchResume(): void;

  setReactionMode(card: string, mode: ReactionModeSetting): void;
  setReactionIntent(triggers: string[]): void;
  passReactionHold(): void;

  changeHistory(action: HistoryChange): Promise<void>;
  fetchReplay(): Promise<{ text: string; filename: string }>;

  queryReach(systemId: string): Promise<ReachReply>;
  previewSecondary(
    card: string,
    primary: string,
    answers: readonly string[],
  ): Promise<SecondaryPreviewReply>;

  /** Needs `capabilities.turnRedo`. */
  fetchTurnRedoStatus(): Promise<TurnRedoStatus | null>;
  turnRedoCommand(command: TurnRedoCommand): Promise<void>;
  /** Needs `capabilities.splicePreview`. Not used by the hook; the history panel calls it. */
  previewSplice?(edit: SpliceEdit): Promise<SplicePreview>;
  /**
   * A question of the engine that plays the game, when it runs on this device (`{"query": ...}`:
   * the odds of a combat). Without it the page asks its own engine.
   */
  queryEngine?(request: object): Promise<unknown>;
}

/** The WebSocket/HTTP transport to the game server. `GameSessionClient` is its implementation. */
export type ServerTransport = GameTransport;

/**
 * State holder for a transport that receives {@link ServerMessage} objects (a local engine
 * host, a test): `ingest` runs the shared reducer and notifies subscribers.
 */
export class MessageDrivenState {
  private state: GameSessionState;
  private readonly listeners = new Set<Listener>();

  constructor(initial: GameSessionState) {
    this.state = initial;
  }

  get(): GameSessionState {
    return this.state;
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  /** Replace the state wholesale (status changes, errors that are not server messages). */
  set(next: GameSessionState): void {
    this.state = next;
    this.listeners.forEach((listener) => listener());
  }

  ingest(message: ServerMessage): void {
    this.set(reduceServerMessage(this.state, message));
  }
}

