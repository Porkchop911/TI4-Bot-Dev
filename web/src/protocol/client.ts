import {
  backoffDelay,
  describeError,
  fetchWithRetry,
  GONE_MESSAGE,
  TERMINATED_MESSAGE,
  isNetworkError,
  onResume,
  type ResumeEvent,
  ServerUnreachableError,
  UNREACHABLE_MESSAGE,
} from "./resilience.ts";
import {
  ClientMessage,
  InitialSnapshotMsg,
  PendingChoiceDto,
  PROTOCOL_VERSION,
  PublicTurnStatus,
  ServerMessage,
  StateUpdateMsg,
  ViewerRole,
  HistoryStatus,
  ReactionModeSetting,
  ReactionIntentStateMsg,
  PreviewRefusal,
  SecondaryPreviewBody,
  ReachOrigin,
  ReachRefusal,
} from "./types.ts";
import { readDeclaredTriggers, writeDeclaredTriggers } from "./bluffTriggers.ts";
import {
  decodeDecisionTrigger,
  decodeInitialSnapshot,
  decodeServerMessage,
  isStaleServerMessage,
} from "./decode.ts";
import { SERVER_CAPABILITIES } from "./capabilities.ts";
import type { GameTransport } from "./transport.ts";
import { decodeSplicePreview, type SpliceEdit, type SplicePreview } from "./splice.ts";
import {
  decodeTurnRedoStatusResponse,
  type TurnRedoCommand,
  type TurnRedoStatus,
} from "./turnRedo.ts";

/** Server messages of the planning/draft protocol, which this client does not use. */
const IGNORED_SERVER_MESSAGE_TYPES: ReadonlySet<string> = new Set([
  "planning_status",
  "planning_update",
  "planning_result",
  "draft_outcome",
  "draft_applied",
]);

/** What the server answered to a `preview_secondary` request. */
export type SecondaryPreviewReply =
  | {
      kind: "preview";
      body: SecondaryPreviewBody;
      /** The game version and decision count the answer was computed at. */
      asOfVersion: number;
      asOfDecisions: number;
    }
  | { kind: "refused"; reason: PreviewRefusal; detail: string };

/** What the server answered to a `query_reach` request ("who can go here"). */
export type ReachReply =
  | { kind: "reach"; systemId: string; asOfVersion: number; origins: ReachOrigin[] }
  | { kind: "refused"; reason: ReachRefusal };

/** The server does not know the reach query (an older build): the tab says so. */
export class ReachUnsupportedError extends Error {
  constructor(message = "This server cannot answer who can go there") {
    super(message);
    this.name = "ReachUnsupportedError";
  }
}

/** The server does not know the preview message (an older build): callers fall back to estimates. */
export class PreviewUnsupportedError extends Error {
  constructor(message = "This server cannot preview a secondary") {
    super(message);
    this.name = "PreviewUnsupportedError";
  }
}

/** How long a preview request may take before the caller falls back to its estimate. */
export const PREVIEW_TIMEOUT_MS = 4_000;

export type ConnectionStatus =
  "connecting" | "connected" | "disconnected" | "error";
export type SnapshotState = InitialSnapshotMsg | StateUpdateMsg;
export type GameLogEntry = import("./types.ts").GameEvent;
export type HistoryChange =
  | "undo"
  | "undo_batch"
  | "undo_pipeline"
  | "redo"
  | "redo_batch"
  | "redo_pipeline"
  | { eventId: string }
  | { cursor: number };

export type MovementStep =
  | { kind: "move"; origin: string; unit: string; damaged: boolean }
  | {
      kind: "load";
      origin: string;
      unit: string;
      source: string | null;
      damaged: boolean;
    }
  | { kind: "done_loading" }
  | { kind: "done_moving" };
export type BasketPlan =
  | {
      kind: "payment";
      steps: ({ kind: "exhaust"; planet: string } | { kind: "trade_good" })[];
    }
  | {
      kind: "agenda_vote_planets";
      steps: (
        { kind: "vote_planet"; planet: string } | { kind: "done_voting" }
      )[];
    }
  | {
      kind: "production";
      destination: string;
      steps: (
        | { kind: "produce"; unit: string; count: number; exchange?: boolean }
        | { kind: "done_producing" }
      )[];
    }
  | {
      kind: "casualties";
      steps: import("../presentation/hitAssignment.ts").CasualtyStep[];
    }
  | {
      kind: "tokens";
      steps: import("../presentation/commandTokens.ts").TokenStep[];
    }
  | {
      /** Diplomacy: the planets to ready, one `ready_planet` step per engine question, in order. */
      kind: "ready_planets";
      steps: { kind: "ready_planet"; planet: string }[];
    };

export type BatchPlan =
  | BasketPlan
  | { kind: "tactical_movement"; destination: string; steps: MovementStep[] };

/**
 * A plan the server stopped part-way because a reaction window opened between its steps. The
 * applied steps are committed and the window waits for its holder; `plan` is what was left. It is
 * sent again only when the player asks, and the server re-checks it against the offers then.
 */
export interface BatchResume {
  plan: BatchPlan;
  /** Planned steps the server applied before it stopped. */
  applied: number;
  /** What the engine is waiting on now. */
  waiting: { subtype: string | null; ownSeat: boolean };
}

/** Decision subtypes each plan kind is answered through. */
export const PLAN_SUBTYPES: Record<BatchPlan["kind"], string[]> = {
  tactical_movement: ["movement_step"],
  payment: ["pay_resources", "pay_influence"],
  agenda_vote_planets: ["vote_exhaust_planet"],
  production: ["produce_unit"],
  casualties: ["sustain_damage", "assign_casualty", "assign_ground_casualty"],
  tokens: ["gain_command_token", "buy_token_with_influence", "pay_influence"],
  ready_planets: ["ready_planet"],
};

/**
 * Keeps a paused plan only while it can still be continued: the engine is asking for a reaction,
 * or is back at a decision the plan answers. Anything else means the game moved on.
 */
function withFinalScores(
  snapshot: SnapshotState,
  scores: Record<string, number>,
): SnapshotState {
  return {
    ...snapshot,
    view: {
      ...snapshot.view,
      finished: true,
      players: snapshot.view.players.map((p) =>
        typeof scores[p.id] === "number" ? { ...p, victory_points: scores[p.id] } : p,
      ),
    },
  };
}

export function settleBatchResume(state: GameSessionState): GameSessionState {
  const resume = state.batchResume;
  if (!resume) return state;
  const subtype = state.pendingChoice?.context?.subtype;
  if (!state.pendingChoice || !subtype) return state;
  if (subtype.startsWith("reaction_") || subtype.startsWith("play_reaction_")) return state;
  if (PLAN_SUBTYPES[resume.plan.kind].includes(subtype)) return state;
  return { ...state, batchResume: null };
}

/** Whether the paused plan can be sent again now: its own seat is back at a decision it answers. */
export function canContinueBatch(
  resume: BatchResume,
  pending: { actor: string; context?: { subtype: string } } | null,
  seat: string | null | undefined,
): boolean {
  return Boolean(
    pending &&
      seat &&
      pending.actor === seat &&
      pending.context &&
      PLAN_SUBTYPES[resume.plan.kind].includes(pending.context.subtype),
  );
}

const HISTORY_RETRY_ATTEMPTS = 20;

/** The server's 409 phrases for "the game moved while this history change was prepared". */
const HISTORY_CONFLICT_PHRASES = [
  "Game advanced or a decision is in flight",
  "Game advanced during rewind",
];

/** True for a 409 that only means the version moved (a bot or an engine step), so a retry with the fresh version is right. */
export function isHistoryVersionConflict(reason: string): boolean {
  return HISTORY_CONFLICT_PHRASES.some((phrase) => reason.includes(phrase));
}

/** What the player reads when a history change gave up: plain words instead of the raw 409. */
export function historyFailureMessage(
  label: string,
  status: number,
  reason: string,
  verb = "changing history",
): string {
  if (status === 409 && isHistoryVersionConflict(reason)) {
    return `The game moved on while ${verb} — try again`;
  }
  return `${label} failed (${status}): ${reason}`;
}
/** Consecutive failed (re)connects before the page stops retrying by itself and asks for a Retry. */
export const RECONNECT_GIVE_UP_AFTER = 8;
/** A socket that has not opened after this long is dead (a tunnel that dropped without a reset). */
const SOCKET_CONNECT_TIMEOUT_MS = 10_000;
/** Back after at least this long away: the socket may be a zombie, so replace it. */
export const RESUME_FORCE_AFTER_MS = 3_000;
/** A submit the server never acknowledges is abandoned after this long, so a click can re-send. */
const SUBMISSION_TIMEOUT_MS = 10_000;

export interface GameSessionState {
  status: ConnectionStatus;
  gameVersion: number;
  snapshot: SnapshotState | null;
  pendingChoice: PendingChoiceDto | null;
  turnStatus: PublicTurnStatus | null;
  lastError: string | null;
  /**
   * Set when automatic reconnecting gave up or the game is gone. The page shows an actionable
   * message (Retry / back to the start page) instead of a raw network error.
   */
  fatal?: { kind: "gone" | "unreachable"; message: string } | null;
  /** From the loss of a connection until the server answers on the new one (a socket that opens and drops again still counts). */
  reconnecting?: boolean;
  /** This seat's own bluff settings, once the server has sent them (never for spectators). */
  reactionIntent?: ReactionIntentStateMsg | null;
  events: GameLogEntry[];
  history: HistoryStatus;
  /** A plan the server paused at a reaction window; see {@link BatchResume}. */
  batchResume?: BatchResume | null;
}

export interface GameSessionClientOptions {
  gameId: string;
  viewer: ViewerRole;
  serverUrl?: string;
}

type Listener = () => void;

const initialState: GameSessionState = {
  status: "connecting",
  gameVersion: 0,
  snapshot: null,
  pendingChoice: null,
  turnStatus: null,
  lastError: null,
  fatal: null,
  reconnecting: false,
  events: [],
  history: { cursor: 0, redo_count: 0 },
};

/** Keep the complete authoritative history, including early rounds and batches. */
export function serverEventLog(
  entries: readonly GameLogEntry[] | undefined,
): GameLogEntry[] {
  return [...(entries ?? [])];
}

const eventIds = new WeakMap<GameLogEntry[], Set<string>>();
function idsFor(entries: GameLogEntry[]): Set<string> {
  let ids = eventIds.get(entries);
  if (!ids) {
    ids = new Set(entries.map((entry) => entry.id));
    eventIds.set(entries, ids);
  }
  return ids;
}

function rejectionMessage(
  message: Extract<ServerMessage, { type: "action_rejected" }>,
): string {
  switch (message.reason.reason) {
    case "stale_version":
      return `Rejected: Stale version (expected ${message.reason.expected}, server at ${message.reason.current})`;
    case "stale_nonce":
      return "Rejected: Stale decision nonce";
    case "unauthorized_seat":
      return "Rejected: Unauthorized seat";
    case "unknown_option":
      return `Rejected: Unknown option '${message.reason.option_id}'`;
    case "no_pending_choice":
      return "Rejected: No decision is currently pending";
    case "validation_failed":
      return `Rejected: ${message.reason.message}`;
  }
}

function pendingChoice(
  envelope: import("./types.ts").PendingChoiceEnvelope,
): PendingChoiceDto {
  if (!envelope.choice) return envelope as unknown as PendingChoiceDto;
  const context = envelope.choice.context;
  return {
    nonce: envelope.nonce,
    actor: envelope.choice.player,
    prompt: envelope.choice.prompt,
    options: envelope.choice.options,
    // The trigger is display data: keep a well-formed one, drop a damaged one.
    context:
      context && "trigger" in context
        ? { ...context, trigger: decodeDecisionTrigger(context.trigger) }
        : context,
    ...(envelope.choice.details ? { details: envelope.choice.details } : {}),
  };
}

/** Applies only validated, non-stale protocol messages to the client projection. */
export function reduceServerMessage(
  state: GameSessionState,
  message: ServerMessage,
): GameSessionState {
  if (isStaleServerMessage(message, state.gameVersion)) return state;

  switch (message.type) {
    case "initial_snapshot":
    case "state_update":
      return {
        ...state,
        snapshot: message,
        gameVersion: message.game_version,
        turnStatus: message.turn_status,
        pendingChoice: message.pending_choice
          ? pendingChoice(message.pending_choice)
          : null,
        events:
          message.type === "initial_snapshot"
            ? serverEventLog(message.events)
            : state.events,
        history: message.history ?? state.history,
      };
    case "event":
      if (idsFor(state.events).has(message.entry.id)) return state;
      const nextEvents = [...state.events, message.entry];
      eventIds.set(nextEvents, idsFor(state.events).add(message.entry.id));
      return {
        ...state,
        events: nextEvents,
        history:
          message.entry.decision_count === undefined
            ? state.history
            : {
                ...state.history,
                cursor: Math.max(
                  state.history.cursor,
                  message.entry.decision_count,
                ),
                redo_count: 0,
              },
      };
    case "pending_choice":
      return {
        ...state,
        gameVersion: message.game_version,
        pendingChoice: pendingChoice({
          nonce: message.nonce,
          choice: message.choice,
        }),
      };
    case "turn_status":
      return {
        ...state,
        gameVersion: message.game_version,
        turnStatus: message.status,
        // The server sends TurnStatus instead of PendingChoice to every non-actor.
        // A previous actor must not retain an actionable choice during a nested window.
        pendingChoice: null,
      };
    case "action_accepted":
      return { ...state, lastError: null };
    case "action_rejected":
      return { ...state, lastError: rejectionMessage(message) };
    case "error":
      return { ...state, lastError: `Server Error: ${message.message}` };
    case "game_over":
      // The server pushes this when the game ends and then no more turn statuses. Ignoring it left
      // every open tab on the last phase banner until it was reloaded (found by the endgame smoke
      // preset).
      return {
        ...state,
        gameVersion: message.game_version,
        turnStatus: { kind: "game_over", winner: message.winner ?? null },
        pendingChoice: null,
        // The last state_update can predate the final score (the winning point is scored by the
        // action that ended the game), so the final scores replace the players' victory points.
        snapshot: state.snapshot ? withFinalScores(state.snapshot, message.final_scores) : state.snapshot,
      };
    case "pong":
      return state;
    case "reaction_intent_state":
      return { ...state, reactionIntent: message };
    case "secondary_preview":
    case "reach_into":
      // Answered to the asking call, never part of the shared projection.
      return state;
  }
}

/** Owns every network ingress point and the lifecycle of one game-session connection. */
export class GameSessionClient implements GameTransport {
  readonly capabilities = SERVER_CAPABILITIES;
  private state = initialState;
  private readonly listeners = new Set<Listener>();
  private socket: WebSocket | null = null;
  private stopped = false;
  private submission: {
    nonce: string;
    optionId: string;
    version: number;
    accepted: boolean;
    promise: Promise<void>;
    resolve: () => void;
    reject: (error: Error) => void;
  } | null = null;
  // An engine step may offer another human reaction before acknowledging the
  // previous choice. Keep its promise until the step commits, but allow the
  // newly offered choice to be submitted meanwhile.
  private priorSubmissions: NonNullable<GameSessionClient["submission"]>[] = [];
  private heartbeat: ReturnType<typeof setInterval> | null = null;
  private retry: ReturnType<typeof setTimeout> | null = null;
  private connectTimer: ReturnType<typeof setTimeout> | null = null;
  /** Consecutive reconnect attempts that did not reach an open socket. */
  private reconnectAttempt = 0;
  private stopResumeWatch: (() => void) | null = null;
  /** Bumped by every (re)load so an older, slower snapshot answer cannot overwrite a newer one. */
  private loadSeq = 0;
  private pingSequence = 0;
  private pendingBatch: {
    nonce: string;
    plan: string;
    requestId: string;
  } | null = null;
  /** The batch being sent now (see `submitBatch`); cleared when its request settles. */
  private batchInFlight: {
    nonce: string;
    plan: string;
    promise: Promise<void>;
  } | null = null;
  private previewSeq = 0;
  /** Set once an older server answered the preview message with "unknown message type". */
  private previewUnsupported = false;
  private readonly previews = new Map<
    number,
    {
      resolve: (reply: SecondaryPreviewReply) => void;
      reject: (error: Error) => void;
      timer: ReturnType<typeof setTimeout>;
    }
  >();

  private reachSeq = 0;
  /** Set once an older server answered the reach query with "unknown message type". */
  private reachUnsupported = false;
  private readonly reaches = new Map<
    number,
    {
      resolve: (reply: ReachReply) => void;
      reject: (error: Error) => void;
      timer: ReturnType<typeof setTimeout>;
    }
  >();

  constructor(private readonly options: GameSessionClientOptions) {}

  /**
   * Read-only: which ships could move into `systemId` if it were activated now, from the engine's
   * own movement search. Open to every viewer. Rejects with {@link ReachUnsupportedError} for an
   * older server (remembered for the life of this client) and with an Error on a timeout or a lost
   * connection.
   */
  queryReach(systemId: string): Promise<ReachReply> {
    if (this.reachUnsupported) return Promise.reject(new ReachUnsupportedError());
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN)
      return Promise.reject(new Error("Not connected to the server"));
    const requestId = ++this.reachSeq;
    const message: ClientMessage = {
      type: "query_reach",
      protocol_version: PROTOCOL_VERSION,
      game_id: this.options.gameId,
      request_id: requestId,
      system_id: systemId,
    };
    return new Promise<ReachReply>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.reaches.delete(requestId);
        reject(new Error("The server did not answer in time"));
      }, PREVIEW_TIMEOUT_MS);
      this.reaches.set(requestId, { resolve, reject, timer });
      try {
        this.socket?.send(JSON.stringify(message));
      } catch (error) {
        clearTimeout(timer);
        this.reaches.delete(requestId);
        reject(error instanceof Error ? error : new Error(String(error)));
      }
    });
  }

  private settleReaches(error: Error): void {
    for (const pending of this.reaches.values()) {
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.reaches.clear();
  }

  /**
   * Read-only: what this seat would be asked for the secondary in progress (the strategy card
   * `card`, played by `primary`) if its window opened right now, after the `answers` already given
   * (the first answers the window question, the rest the follow-up questions in order). The server
   * changes nothing and answers this connection only. Rejects with {@link PreviewUnsupportedError}
   * for an older server (remembered for the life of this client) and with an Error on a timeout or
   * a lost connection; callers then use their own estimate.
   */
  previewSecondary(
    card: string,
    primary: string,
    answers: readonly string[],
  ): Promise<SecondaryPreviewReply> {
    if (this.options.viewer.role !== "player") return Promise.reject(new PreviewUnsupportedError());
    if (this.previewUnsupported) return Promise.reject(new PreviewUnsupportedError());
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN)
      return Promise.reject(new Error("Not connected to the server"));
    const requestId = ++this.previewSeq;
    const message: ClientMessage = {
      type: "preview_secondary",
      protocol_version: PROTOCOL_VERSION,
      game_id: this.options.gameId,
      request_id: requestId,
      card,
      primary,
      answers: [...answers],
    };
    return new Promise<SecondaryPreviewReply>((resolve, reject) => {
      const timer = setTimeout(() => {
        this.previews.delete(requestId);
        reject(new Error("The server did not answer the preview in time"));
      }, PREVIEW_TIMEOUT_MS);
      this.previews.set(requestId, { resolve, reject, timer });
      try {
        this.socket?.send(JSON.stringify(message));
      } catch (error) {
        clearTimeout(timer);
        this.previews.delete(requestId);
        reject(error instanceof Error ? error : new Error(String(error)));
      }
    });
  }

  private settlePreviews(error: Error): void {
    for (const pending of this.previews.values()) {
      clearTimeout(pending.timer);
      pending.reject(error);
    }
    this.previews.clear();
  }

  getState(): GameSessionState {
    return this.state;
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  start(): void {
    this.stopped = false;
    this.reconnectAttempt = 0;
    this.setState({ ...this.state, status: "connecting", lastError: null, fatal: null });
    void this.loadSnapshot();
    this.openSocket();
    this.stopResumeWatch?.();
    this.stopResumeWatch = onResume((event) => this.resume(event));
  }

  /**
   * The page is back (tab visible, bfcache restore, network online, focus) or the user pressed
   * Retry: re-read the state and reconnect without waiting for the old socket to notice it died.
   * A short absence with a healthy socket changes nothing.
   */
  resume(event: Pick<ResumeEvent, "reason"> & Partial<ResumeEvent> = { reason: "focus" }): void {
    if (this.stopped) return;
    const open = this.socket?.readyState === WebSocket.OPEN;
    const away = event.awayMs ?? Infinity;
    if (open && !this.state.fatal && away < RESUME_FORCE_AFTER_MS && event.reason !== "online") return;
    this.reconnect();
  }

  /** User-initiated: try again after the page gave up. */
  retryNow(): void {
    if (!this.stopped) this.reconnect();
  }

  private reconnect(): void {
    this.reconnectAttempt = 0;
    this.clearTimers();
    this.detachSocket();
    const stale = /^(Snapshot request failed|WebSocket network error)/.test(this.state.lastError ?? "");
    this.setState({
      ...this.state,
      status: "connecting",
      fatal: null,
      reconnecting: true,
      lastError: stale ? null : this.state.lastError,
    });
    void this.loadSnapshot();
    this.openSocket();
  }

  stop(): void {
    this.stopped = true;
    this.stopResumeWatch?.();
    this.stopResumeWatch = null;
    this.batchInFlight = null;
    this.clearTimers();
    this.detachSocket();
    this.rejectSubmission("Submission stopped");
    this.settlePreviews(new Error("Session stopped"));
    this.settleReaches(new Error("Session stopped"));
  }

  async submitChoice(optionId: string): Promise<void> {
    const { pendingChoice, gameVersion } = this.state;
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
      const message = "Cannot submit choice: not connected to server (reconnecting)";
      this.setState({ ...this.state, lastError: message });
      throw new Error(message);
    }
    if (!pendingChoice) {
      const message = "No decision currently pending";
      this.setState({ ...this.state, lastError: message });
      throw new Error(message);
    }
    if (this.submission) {
      if (
        this.submission.nonce === pendingChoice.nonce &&
        this.submission.optionId === optionId
      )
        return this.submission.promise;
      if (this.submission.nonce === pendingChoice.nonce)
        throw new Error("Another choice submission is still pending");
      this.priorSubmissions.push(this.submission);
      this.submission = null;
    }

    const message: ClientMessage = {
      type: "submit_choice",
      protocol_version: PROTOCOL_VERSION,
      game_id: this.options.gameId,
      nonce: pendingChoice.nonce,
      expected_version: gameVersion,
      option_id: optionId,
    };
    let resolve!: () => void;
    let reject!: (error: Error) => void;
    let timeout: ReturnType<typeof setTimeout> | undefined;
    const promise = new Promise<void>((done, fail) => {
      resolve = () => {
        clearTimeout(timeout);
        done();
      };
      reject = (error) => {
        clearTimeout(timeout);
        fail(error);
      };
    });
    this.submission = {
      nonce: pendingChoice.nonce,
      optionId,
      version: gameVersion,
      accepted: false,
      promise,
      resolve,
      reject,
    };
    try {
      this.socket.send(JSON.stringify(message));
    } catch (error) {
      this.rejectSubmission(`Could not send choice: ${String(error)}`);
    }
    // Without this, a submit that never gets an answer keeps returning the same dead promise to
    // every later click for the same option, and the decision looks frozen.
    timeout = setTimeout(() => {
      if (this.submission?.promise === promise && !this.submission.accepted)
        this.rejectSubmission("No response from the server; try again");
    }, SUBMISSION_TIMEOUT_MS);
    return promise;
  }

  /**
   * Asks the server to stop (or resume) offering one action card to this seat for the rest of
   * the game. The answer is the seat's next state update, which carries the modes; nothing is
   * assumed locally. Rejected when not connected or when watching.
   */
  setReactionMode(card: string, mode: ReactionModeSetting): void {
    if (this.options.viewer.role !== "player") {
      this.setState({ ...this.state, lastError: "Only a seated player can change reaction modes" });
      return;
    }
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
      this.setState({
        ...this.state,
        lastError: "Cannot change the setting: not connected to server",
      });
      return;
    }
    const message: ClientMessage = {
      type: "set_reaction_mode",
      protocol_version: PROTOCOL_VERSION,
      game_id: this.options.gameId,
      card,
      mode,
    };
    try {
      this.socket.send(JSON.stringify(message));
    } catch (error) {
      this.setState({ ...this.state, lastError: `Could not send the setting: ${String(error)}` });
    }
  }

  /**
   * Declares which kinds of reaction window this seat bluffs about (the whole set; empty clears).
   * Kept in this browser per game and seat and sent again on every connect; the server forgets it
   * on restart. The server's answer is the seat's next `reaction_intent_state`.
   */
  setReactionIntent(triggers: string[]): void {
    const viewer = this.options.viewer;
    if (viewer.role !== "player") return;
    writeDeclaredTriggers(this.options.gameId, viewer.seat, triggers);
    this.sendIntent(triggers);
  }

  /** Ends the bluff hold early. Silent when nothing is held. */
  passReactionHold(): void {
    if (this.options.viewer.role !== "player") return;
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) return;
    try {
      this.socket.send(
        JSON.stringify({
          type: "pass_reaction_hold",
          protocol_version: PROTOCOL_VERSION,
          game_id: this.options.gameId,
        } satisfies ClientMessage),
      );
    } catch {
      // The hold ends by itself within seconds.
    }
  }

  private sendIntent(triggers: string[]): void {
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) return;
    try {
      this.socket.send(
        JSON.stringify({
          type: "set_reaction_intent",
          protocol_version: PROTOCOL_VERSION,
          game_id: this.options.gameId,
          triggers,
        } satisfies ClientMessage),
      );
    } catch (error) {
      this.setState({ ...this.state, lastError: `Could not send the setting: ${String(error)}` });
    }
  }

  async submitMovementBatch(
    destination: string,
    steps: MovementStep[],
  ): Promise<void> {
    return this.submitBatch({ kind: "tactical_movement", destination, steps });
  }

  /** Sends what is left of a plan the server paused at a reaction window. */
  async resumeBatch(): Promise<void> {
    const resume = this.state.batchResume;
    if (!resume) throw new Error("There is no paused plan to continue");
    const seat =
      this.options.viewer.role === "player" ? this.options.viewer.seat : null;
    if (!canContinueBatch(resume, this.state.pendingChoice, seat))
      throw new Error("Workflow is no longer pending");
    try {
      await this.submitBatch(resume.plan);
    } catch (error) {
      // The server checked the remainder against the current offers and refused it: stage again.
      if (this.state.batchResume === resume)
        this.setState({
          ...this.state,
          batchResume: null,
          lastError: error instanceof Error ? error.message : String(error),
        });
      throw error;
    }
  }

  dismissBatchResume(): void {
    if (this.state.batchResume) this.setState({ ...this.state, batchResume: null });
  }

  async submitBatch(plan: BatchPlan): Promise<void> {
    if (
      this.options.viewer.role !== "player" ||
      !this.options.viewer.playerSession
    )
      throw new Error("A player session is required");
    const pending = this.state.pendingChoice;
    if (
      !pending ||
      pending.actor !== this.options.viewer.seat ||
      !pending.context
    )
      throw new Error("Decision is no longer pending");
    const expected = PLAN_SUBTYPES[plan.kind];
    if (!expected.includes(pending.context.subtype))
      throw new Error("Workflow is no longer pending");
    const serialized = JSON.stringify(plan);
    // One confirmation per decision at a time. The answer of a batch is only applied to the
    // session when it comes back, so until then this state still shows the decision as open at
    // the old version: a second send (a repeated click, or a prepared Auto answer racing a click)
    // would carry that old version and be refused as stale. The same plan shares the running
    // request; a different one waits for the answer by being refused.
    const running = this.batchInFlight;
    if (running && running.nonce === pending.nonce) {
      if (running.plan === serialized) return running.promise;
      throw new Error("Another confirmation for this decision is still being sent");
    }
    const entry: NonNullable<GameSessionClient["batchInFlight"]> = {
      nonce: pending.nonce,
      plan: serialized,
      promise: Promise.resolve(),
    };
    entry.promise = (async () => {
      try {
        await this.sendBatch(plan, pending, serialized);
      } finally {
        if (this.batchInFlight === entry) this.batchInFlight = null;
      }
    })();
    this.batchInFlight = entry;
    return entry.promise;
  }

  /**
   * After a network error on a batch POST: re-read the game and report whether the decision the
   * batch targeted is gone (applied, or overtaken by someone else). The fresh state is adopted.
   */
  private async batchWasApplied(nonce: string): Promise<boolean> {
    try {
      const response = await fetchWithRetry(
        () => fetch(this.snapshotUrl(), { headers: this.snapshotHeaders() }),
        { attempts: 3, cancelled: () => this.stopped },
      );
      if (!response.ok) return false;
      this.ingestHttpSnapshot(await response.json());
      return this.state.pendingChoice?.nonce !== nonce;
    } catch {
      return false;
    }
  }

  private async sendBatch(
    plan: BatchPlan,
    pending: NonNullable<GameSessionClient["state"]["pendingChoice"]>,
    serialized: string,
  ): Promise<void> {
    if (
      this.pendingBatch?.nonce !== pending.nonce ||
      this.pendingBatch.plan !== serialized
    )
      this.pendingBatch = {
        nonce: pending.nonce,
        plan: serialized,
        requestId: crypto.randomUUID(),
      };
    const sentAtVersion = this.state.gameVersion;
    let response: Response;
    try {
      response = await fetch(
        this.snapshotUrl().replace(/\/snapshot$/, "/batches"),
        {
          method: "POST",
          headers: {
            ...this.snapshotHeaders(),
            "content-type": "application/json",
          },
          body: JSON.stringify({
            request_id: this.pendingBatch.requestId,
            expected_version: this.state.gameVersion,
            nonce: pending.nonce,
            plan,
          }),
        },
      );
    } catch (error) {
      if (!isNetworkError(error)) throw error;
      // The request may or may not have reached the server. Never re-send blindly: look at the
      // server's state. The request id is kept, so a later confirmation of the same plan is
      // recognised by the server as the same request.
      if (await this.batchWasApplied(pending.nonce)) return;
      throw new Error(
        "The connection dropped before the server confirmed. Nothing was applied; confirm again.",
      );
    }
    if (!response.ok) {
      if (
        response.status !== 500 &&
        response.status !== 502 &&
        response.status !== 503
      )
        this.pendingBatch = null;
      const body = await response.text();
      let failure: {
        failed_step?: number;
        reason?: string;
        expected?: string;
        message?: string;
      };
      try {
        failure = JSON.parse(body);
      } catch {
        failure = { message: body || `HTTP ${response.status}` };
      }
      // A refusal as stale that a newer ACCEPTED version has already overtaken (the answer that
      // beat this one is applied: a prepared Auto answer racing a click) has nothing left to
      // report: the decision this request targeted is gone. No error; the live state is current.
      if (
        response.status === 409 &&
        /stale decision boundary/.test(body) &&
        this.state.gameVersion > sentAtVersion
      )
        return;
      // The server explains the rejection in `message`; older servers only send the reason.
      throw new Error(
        failure.message
          ? `Batch rejected: ${failure.message}`
          : `Batch step ${(failure.failed_step ?? 0) + 1}: ${failure.reason ?? "batch rejected"}${failure.expected ? ` (${failure.expected})` : ""}`,
      );
    }
    this.pendingBatch = null;
    const result = (await response.json()) as {
      snapshot: unknown;
      active?: boolean;
      interrupted?: {
        applied_steps: number;
        remaining_steps: unknown[];
        offered?: { subtype?: string | null; own_seat?: boolean };
      };
    };
    if (result.active === false)
      throw new Error(
        "This confirmation was already committed but is now undone. Refresh the decision before confirming again.",
      );
    const snapshot = decodeInitialSnapshot(
      { type: "initial_snapshot", ...(result.snapshot as object) },
      this.options.gameId,
    );
    // A reaction window opened between planned steps: what was applied is kept, the window waits
    // for its holder, and the rest of the plan is offered again once it resolves.
    const stopped = result.interrupted?.remaining_steps.length
      ? result.interrupted
      : undefined;
    this.rejectSubmission("Game history changed");
    this.detachSocket();
    this.clearTimers();
    this.setState(
      reduceServerMessage(
        {
          ...this.state,
          pendingChoice: null,
          lastError: null,
          batchResume: stopped
            ? {
                plan: { ...plan, steps: stopped.remaining_steps } as BatchPlan,
                applied: stopped.applied_steps,
                waiting: {
                  subtype: stopped.offered?.subtype ?? null,
                  ownSeat: stopped.offered?.own_seat ?? false,
                },
              }
            : null,
        },
        { ...snapshot, type: "initial_snapshot" },
      ),
    );
    this.openSocket();
  }

  /** Any seated player: the live game history (seed, seats, decisions, events) as pretty JSON. */
  async fetchReplay(): Promise<{ text: string; filename: string }> {
    if (this.options.viewer.role !== "player" || !this.options.viewer.playerSession)
      throw new Error("A player session is required");
    const response = await fetchWithRetry(
      () => fetch(this.snapshotUrl().replace(/\/snapshot$/, "/replay"), { headers: this.snapshotHeaders() }),
      { attempts: 3, cancelled: () => this.stopped },
    );
    if (!response.ok) {
      const reason = (await response.text().catch(() => "")).trim();
      throw new Error(reason || `The server refused the replay (${response.status})`);
    }
    const replay: unknown = await response.json();
    return {
      text: JSON.stringify(replay, null, 2),
      filename: `ti4-replay-${this.options.gameId}.json`,
    };
  }

  /**
   * Host only, read-only: how would the game fare if one past decision were removed or changed
   * and the rest replayed from the same seed? Changes nothing on the server.
   */
  async previewSplice(edit: SpliceEdit): Promise<SplicePreview> {
    if (this.options.viewer.role !== "player" || !this.options.viewer.playerSession)
      throw new Error("A player session is required");
    const url = this.snapshotUrl().replace(/\/snapshot$/, "/history/splice-preview");
    const response = await fetch(url, {
      method: "POST",
      headers: { ...this.snapshotHeaders(), "content-type": "application/json" },
      body: JSON.stringify({ edit }),
    });
    if (!response.ok) {
      const reason = (await response.text().catch(() => "")).trim();
      throw new Error(reason || `The server refused the splice preview (${response.status})`);
    }
    return decodeSplicePreview(await response.json());
  }

  /** The host changes the authoritative Rust timeline; all clients reconnect to it. */
  async changeHistory(action: HistoryChange): Promise<void> {
    const body =
      typeof action === "string"
        ? { action }
        : "cursor" in action
          ? { action: "restore_cursor", cursor: action.cursor }
          : { action: "restore", event_id: action.eventId };
    const name = typeof action === "string" ? action : "restore";
    const verb = name.startsWith("redo") ? "redoing" : name.startsWith("undo") ? "undoing" : "rewinding";
    await this.replaceHistory("/history", body, "History change", verb);
  }

  /**
   * Any seated player (the host for another seat): where a turn redo stands, or null when none is
   * in flight. The server drops an original that can no longer be restored when this is read.
   */
  async fetchTurnRedoStatus(): Promise<TurnRedoStatus | null> {
    if (this.options.viewer.role !== "player" || !this.options.viewer.playerSession)
      throw new Error("A player session is required");
    const response = await fetchWithRetry(
      () => fetch(this.snapshotUrl().replace(/\/snapshot$/, "/turn-redo"), { headers: this.snapshotHeaders() }),
      { attempts: 3, cancelled: () => this.stopped },
    );
    if (!response.ok) {
      const reason = (await response.text().catch(() => "")).trim();
      throw new Error(reason || `The server refused the turn redo status (${response.status})`);
    }
    return decodeTurnRedoStatusResponse(await response.json());
  }

  /**
   * Turn redo: rewind a seat's last turn(s), replay the round after the new turn, restore the
   * original timeline, or keep the new one. Every action but `keep` replaces the timeline for
   * everyone, so all clients reconnect to it.
   */
  async turnRedoCommand(command: TurnRedoCommand): Promise<void> {
    if (command.action === "keep") {
      if (this.options.viewer.role !== "player" || !this.options.viewer.playerSession)
        throw new Error("A player session is required");
      const response = await fetch(this.snapshotUrl().replace(/\/snapshot$/, "/turn-redo"), {
        method: "POST",
        headers: { ...this.snapshotHeaders(), "content-type": "application/json" },
        body: JSON.stringify({ action: "keep", expected_version: this.state.gameVersion }),
      });
      if (!response.ok) {
        const reason = (await response.text().catch(() => "")).trim();
        throw new Error(
          reason || `The server refused to keep the new timeline (${response.status})`,
        );
      }
      return;
    }
    await this.replaceHistory("/turn-redo", { ...command }, "Turn redo", "redoing the turn");
  }

  /**
   * POST a timeline replacement (`/history` or `/turn-redo`) and switch to the snapshot of the
   * replacement session. Retries while the server is still advancing toward its next human choice.
   */
  private async replaceHistory(
    path: string,
    body: Record<string, unknown>,
    label: string,
    verb?: string,
  ): Promise<void> {
    if (
      this.options.viewer.role !== "player" ||
      !this.options.viewer.playerSession
    )
      throw new Error("A player session is required");
    const url = this.snapshotUrl().replace(/\/snapshot$/, path);
    let version = this.state.gameVersion;
    let response!: Response;
    let conflictReason: string | undefined;
    for (let attempt = 0; attempt < HISTORY_RETRY_ATTEMPTS; attempt++) {
      conflictReason = undefined;
      response = await fetch(url, {
        method: "POST",
        headers: {
          ...this.snapshotHeaders(),
          "content-type": "application/json",
        },
        body: JSON.stringify({ ...body, expected_version: version }),
      });
      if (response.ok || response.status !== 409) break;
      conflictReason = await response.text();
      if (
        attempt === HISTORY_RETRY_ATTEMPTS - 1 ||
        !isHistoryVersionConflict(conflictReason)
      )
        break;
      // The worker may still be advancing automatically toward its next human choice, or a bot
      // acted while the request was being prepared (the history cursor moves with bots playing,
      // so it is not a reason to give up). Re-read the version and issue the change again.
      await new Promise((resolve) => setTimeout(resolve, 100));
      const latest = await fetch(this.snapshotUrl(), {
        headers: this.snapshotHeaders(),
      });
      if (!latest.ok) break;
      const snapshot = decodeInitialSnapshot(
        await latest.json(),
        this.options.gameId,
      );
      version = snapshot.game_version;
    }
    if (!response.ok) {
      const reason = conflictReason ?? (await response.text());
      const error = historyFailureMessage(label, response.status, reason, verb);
      this.setState({ ...this.state, lastError: error });
      throw new Error(error);
    }
    const snapshot = decodeInitialSnapshot(
      await response.json(),
      this.options.gameId,
    );
    const expected = this.options.viewer;
    if (
      snapshot.viewer.role !== expected.role ||
      (expected.role === "player" &&
        (snapshot.viewer.role !== "player" ||
          snapshot.viewer.seat !== expected.seat))
    ) {
      throw new Error("Server viewer identity does not match this session");
    }
    // The server answered without replacing the timeline (a repeated turn redo auto-play, or one
    // with nothing to replay): the live connection is still the right one. Tearing it down would
    // throw away an answer in flight ("Submission disconnected...") for nothing.
    const generation = snapshot.history?.generation;
    if (
      generation !== undefined &&
      generation === this.state.history.generation &&
      this.socket !== null
    )
      return;
    this.rejectSubmission("Game history changed");
    this.detachSocket();
    this.clearTimers();
    this.setState(
      reduceServerMessage(
        { ...this.state, pendingChoice: null, lastError: null, batchResume: null },
        { ...snapshot, type: "initial_snapshot" },
      ),
    );
    this.openSocket();
  }

  /**
   * Reads the authoritative state over HTTP. A plain GET, so it is retried with backoff while the
   * network fails or the proxy answers 502/503/504; a 404 means the game is gone.
   */
  private async loadSnapshot(): Promise<void> {
    const seq = ++this.loadSeq;
    try {
      const response = await fetchWithRetry(
        () => fetch(this.snapshotUrl(), { headers: this.snapshotHeaders() }),
        { attempts: 4, cancelled: () => this.stopped || seq !== this.loadSeq },
      );
      if (this.stopped || seq !== this.loadSeq) return;
      if (response.status === 404) {
        this.giveUp("gone", GONE_MESSAGE);
        return;
      }
      if (!response.ok)
        throw new Error(
          `Snapshot request failed (${response.status}): ${await response.text().catch(() => "")}`,
        );
      const body = await response.json();
      if (this.stopped || seq !== this.loadSeq) return;
      this.ingestHttpSnapshot(body);
      this.dropStaleBatch();
    } catch (error) {
      if (this.stopped || seq !== this.loadSeq) return;
      // A network failure is the reconnect loop's business (the chip shows); only a real
      // server answer is worth an error message.
      if (error instanceof ServerUnreachableError || isNetworkError(error)) return;
      this.setState({
        ...this.state,
        lastError: `Snapshot request failed: ${describeError(error)}`,
      });
    }
  }

  /** A batch request id only makes sense for the decision it was made for. */
  private dropStaleBatch(): void {
    if (this.pendingBatch && this.state.pendingChoice?.nonce !== this.pendingBatch.nonce)
      this.pendingBatch = null;
  }

  private giveUp(kind: "gone" | "unreachable", message: string): void {
    this.clearTimers();
    this.detachSocket();
    this.setState({ ...this.state, status: "disconnected", reconnecting: false, fatal: { kind, message } });
  }

  private openSocket(): void {
    let socket: WebSocket;
    try {
      socket = new WebSocket(this.webSocketUrl());
    } catch {
      this.scheduleReconnect(null);
      return;
    }
    this.socket = socket;
    // A tunnel that dropped silently never answers the handshake; do not wait for the browser's
    // own (minutes long) timeout.
    this.connectTimer = setTimeout(() => {
      if (this.socket === socket && socket.readyState !== WebSocket.OPEN) {
        this.detachSocket();
        this.scheduleReconnect(null);
      }
    }, SOCKET_CONNECT_TIMEOUT_MS);
    socket.onopen = () => {
      if (this.stopped || this.socket !== socket) return;
      if (this.connectTimer) clearTimeout(this.connectTimer);
      this.connectTimer = null;
      const playerSession =
        this.options.viewer.role === "player"
          ? this.options.viewer.playerSession
          : undefined;
      const message: ClientMessage = {
        type: "subscribe",
        protocol_version: PROTOCOL_VERSION,
        game_id: this.options.gameId,
        player_session: playerSession,
      };
      socket.send(JSON.stringify(message));
      if (playerSession)
        this.heartbeat = setInterval(() => {
          if (socket.readyState === WebSocket.OPEN)
            socket.send(
              JSON.stringify({
                type: "ping",
                protocol_version: PROTOCOL_VERSION,
                sequence: ++this.pingSequence,
              } satisfies ClientMessage),
            );
        }, 10_000);
      this.setState({ ...this.state, status: "connected", lastError: null, fatal: null });
    };
    socket.onmessage = (event) => {
      // The server answered: this connection works, so the next failure starts the backoff over.
      if (this.socket === socket) {
        this.reconnectAttempt = 0;
        if (this.state.reconnecting) this.setState({ ...this.state, reconnecting: false });
      }
      this.ingestWebSocket(event.data);
    };
    socket.onerror = () => {
      // The close event follows and drives the reconnect; a network error is not an error message.
      if (!this.stopped && this.socket === socket && this.state.status !== "connected")
        this.setState({ ...this.state, status: "error" });
    };
    socket.onclose = (event) => {
      if (!this.stopped && this.socket === socket) {
        this.clearTimers();
        this.socket = null;
        this.rejectSubmission("Submission disconnected before confirmation");
        this.settlePreviews(new Error("Disconnected"));
        this.settleReaches(new Error("Disconnected"));
        if (event?.code === 4002) {
          // The host ended the game on purpose; do not reconnect.
          this.giveUp("gone", TERMINATED_MESSAGE);
          return;
        }
        // The last known game stays on screen (and with it every local draft) while reconnecting;
        // the reconnect brings a fresh snapshot, and nothing can be sent without an open socket.
        this.setState({ ...this.state, status: "disconnected", reconnecting: true, reactionIntent: null });
        this.scheduleReconnect(event?.code ?? null);
      }
    };
  }

  /** Reconnects with exponential backoff and jitter; asks the user after too many failures. */
  private scheduleReconnect(code: number | null): void {
    if (this.stopped) return;
    if (code !== 4001) this.reconnectAttempt++;
    if (this.reconnectAttempt > RECONNECT_GIVE_UP_AFTER) {
      this.giveUp("unreachable", UNREACHABLE_MESSAGE);
      return;
    }
    if (this.state.status === "connected" || this.state.status === "connecting" || !this.state.reconnecting)
      this.setState({ ...this.state, status: "disconnected", reconnecting: true });
    this.retry = setTimeout(
      () => {
        if (this.stopped) return;
        void this.loadSnapshot();
        this.openSocket();
      },
      code === 4001 ? 0 : backoffDelay(this.reconnectAttempt),
    );
  }

  private ingestHttpSnapshot(value: unknown): void {
    this.apply(
      decodeInitialSnapshot(value, this.options.gameId) as Extract<
        ServerMessage,
        { type: "initial_snapshot" }
      >,
    );
  }

  private ingestWebSocket(value: unknown): void {
    try {
      const raw: unknown = typeof value === "string" ? JSON.parse(value) : value;
      // An older server does not know the preview message and answers it with a protocol error
      // that names it. That is "no preview available", never an error to show the player.
      if (
        this.previews.size > 0 &&
        typeof raw === "object" &&
        raw !== null &&
        (raw as { type?: unknown }).type === "error" &&
        String((raw as { message?: unknown }).message ?? "").includes("preview_secondary")
      ) {
        this.previewUnsupported = true;
        this.settlePreviews(new PreviewUnsupportedError());
        return;
      }
      if (
        this.reaches.size > 0 &&
        typeof raw === "object" &&
        raw !== null &&
        (raw as { type?: unknown }).type === "error" &&
        String((raw as { message?: unknown }).message ?? "").includes("query_reach")
      ) {
        this.reachUnsupported = true;
        this.settleReaches(new ReachUnsupportedError());
        return;
      }
      // The server pushes the planning/draft protocol to every player connection. This client
      // has no planning UI, so those messages are not for it and must not surface as errors.
      if (
        typeof raw === "object" &&
        raw !== null &&
        IGNORED_SERVER_MESSAGE_TYPES.has(String((raw as { type?: unknown }).type))
      )
        return;
      const message = decodeServerMessage(raw, this.options.gameId);
      if (message.type === "reach_into") {
        const pending = this.reaches.get(message.request_id);
        if (pending) {
          clearTimeout(pending.timer);
          this.reaches.delete(message.request_id);
          pending.resolve(
            message.refused
              ? { kind: "refused", reason: message.refused }
              : {
                  kind: "reach",
                  systemId: message.system_id,
                  asOfVersion: message.as_of_version,
                  origins: message.origins,
                },
          );
        }
        return;
      }
      if (message.type === "secondary_preview") {
        const pending = this.previews.get(message.request_id);
        if (pending) {
          clearTimeout(pending.timer);
          this.previews.delete(message.request_id);
          pending.resolve(
            message.outcome.result === "preview"
              ? {
                  kind: "preview",
                  body: message.outcome.preview,
                  asOfVersion: message.as_of_version,
                  asOfDecisions: message.as_of_decisions,
                }
              : { kind: "refused", reason: message.outcome.reason, detail: message.outcome.detail },
          );
        }
        return;
      }
      this.apply(message);
      if (message.type === "reaction_intent_state") {
        // The server's answer is the truth (it may have refused, or forgotten in a restart).
        const viewer = this.options.viewer;
        if (viewer.role === "player")
          writeDeclaredTriggers(this.options.gameId, viewer.seat, message.triggers);
      }
      if (message.type === "initial_snapshot" && this.options.viewer.role === "player") {
        // Tell the server again what this browser declared; only when something is declared, so
        // a server that predates the feature never sees the message.
        const declared = readDeclaredTriggers(this.options.gameId, this.options.viewer.seat);
        if (declared.length > 0) this.sendIntent(declared);
      }
    } catch (error) {
      if (!this.stopped) {
        const message = `Invalid server message: ${String(error)}`;
        this.setState({ ...this.state, lastError: message });
        this.rejectSubmission(message);
      }
    }
  }

  private apply(message: ServerMessage): void {
    if (
      message.type === "initial_snapshot" ||
      message.type === "state_update"
    ) {
      const expected = this.options.viewer;
      if (
        message.viewer.role !== expected.role ||
        (expected.role === "player" &&
          (message.viewer.role !== "player" ||
            message.viewer.seat !== expected.seat))
      ) {
        this.setState({
          ...initialState,
          status: "error",
          lastError: "Server viewer identity does not match this session",
        });
        this.stop();
        return;
      }
    }
    // A state update may precede its acknowledgement on the broadcast channel.
    // Do not discard a late acknowledgement just because its version is older.
    if (message.type === "action_accepted") {
      const index = this.priorSubmissions.findIndex(
        (pending) =>
          pending.optionId === message.option_id &&
          message.game_version >= pending.version,
      );
      if (index !== -1) {
        this.priorSubmissions.splice(index, 1)[0].resolve();
        this.setState(reduceServerMessage(this.state, message));
        return;
      }
    }
    if (message.type === "action_rejected") {
      const index = this.priorSubmissions.findIndex(
        (pending) => pending.version === message.game_version,
      );
      if (index !== -1) {
        const reason = rejectionMessage(message);
        this.priorSubmissions.splice(index, 1)[0].reject(new Error(reason));
        this.setState({ ...this.state, lastError: reason });
        return;
      }
    }
    if (
      message.type === "action_accepted" &&
      this.submission &&
      message.option_id === this.submission.optionId &&
      message.game_version >= this.submission.version
    ) {
      this.submission.accepted = true;
    }
    if (message.type === "action_rejected" && this.submission) {
      const reason = rejectionMessage(message);
      this.setState({ ...this.state, lastError: reason });
      this.rejectSubmission(reason);
      return;
    }
    this.setState(reduceServerMessage(this.state, message));
    const submission = this.submission;
    // The worker can announce the next choice at a newer version without sending
    // a state update at that version. That choice is itself confirmation of progress.
    if (
      submission?.accepted &&
      this.state.gameVersion > submission.version &&
      (this.state.snapshot?.game_version === this.state.gameVersion ||
        this.state.pendingChoice !== null) &&
      this.state.pendingChoice?.nonce !== submission.nonce
    ) {
      this.submission = null;
      submission.resolve();
    }
  }

  private rejectSubmission(reason: string): void {
    const submission = this.submission;
    this.submission = null;
    submission?.reject(new Error(reason));
    for (const prior of this.priorSubmissions.splice(0))
      prior.reject(new Error(reason));
  }

  private detachSocket(): void {
    const socket = this.socket;
    this.socket = null;
    if (!socket) return;
    socket.onopen = null;
    socket.onmessage = null;
    socket.onerror = null;
    socket.onclose = null;
    if (
      socket.readyState === WebSocket.CONNECTING ||
      socket.readyState === WebSocket.OPEN
    )
      socket.close();
  }

  private clearTimers(): void {
    if (this.heartbeat) clearInterval(this.heartbeat);
    if (this.retry) clearTimeout(this.retry);
    if (this.connectTimer) clearTimeout(this.connectTimer);
    this.heartbeat = null;
    this.retry = null;
    this.connectTimer = null;
  }

  private setState(next: GameSessionState): void {
    this.state = settleBatchResume(next);
    this.listeners.forEach((listener) => listener());
  }

  private webSocketUrl(): string {
    if (this.options.serverUrl) return this.options.serverUrl;
    const protocol = window.location.protocol === "https:" ? "wss:" : "ws:";
    return `${protocol}//${window.location.host}/ws/games/${encodeURIComponent(this.options.gameId)}`;
  }

  private snapshotUrl(): string {
    if (!this.options.serverUrl)
      return `/api/games/${encodeURIComponent(this.options.gameId)}/snapshot`;
    const url = new URL(this.options.serverUrl, window.location.href);
    url.protocol = url.protocol === "wss:" ? "https:" : "http:";
    url.pathname = `/api/games/${encodeURIComponent(this.options.gameId)}/snapshot`;
    url.search = "";
    return url.toString();
  }

  private snapshotHeaders(): HeadersInit {
    return this.options.viewer.role === "player" &&
      this.options.viewer.playerSession
      ? { "x-ti4-player-session": this.options.viewer.playerSession }
      : {};
  }
}
