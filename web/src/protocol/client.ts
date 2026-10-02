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
} from "./types.ts";
import { decodeInitialSnapshot, decodeServerMessage, isStaleServerMessage } from "./decode.ts";

export type ConnectionStatus = "connecting" | "connected" | "disconnected" | "error";
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
  | { kind: "load"; origin: string; unit: string; source: string | null; damaged: boolean }
  | { kind: "done_loading" }
  | { kind: "done_moving" };
export type BasketPlan =
  | { kind: "payment"; steps: ({ kind: "exhaust"; planet: string } | { kind: "trade_good" })[] }
  | {
      kind: "agenda_vote_planets";
      steps: ({ kind: "vote_planet"; planet: string } | { kind: "done_voting" })[];
    }
  | {
      kind: "production";
      destination: string;
      steps: ({ kind: "produce"; unit: string; count: number } | { kind: "done_producing" })[];
    };

const HISTORY_RETRY_ATTEMPTS = 20;

export interface GameSessionState {
  status: ConnectionStatus;
  gameVersion: number;
  snapshot: SnapshotState | null;
  pendingChoice: PendingChoiceDto | null;
  turnStatus: PublicTurnStatus | null;
  lastError: string | null;
  events: GameLogEntry[];
  history: HistoryStatus;
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
  events: [],
  history: { cursor: 0, redo_count: 0 },
};

/** Keep the complete authoritative history, including early rounds and batches. */
export function serverEventLog(entries: readonly GameLogEntry[] | undefined): GameLogEntry[] {
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

function rejectionMessage(message: Extract<ServerMessage, { type: "action_rejected" }>): string {
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

function pendingChoice(envelope: import("./types.ts").PendingChoiceEnvelope): PendingChoiceDto {
  if (!envelope.choice) return envelope as unknown as PendingChoiceDto;
  return {
    nonce: envelope.nonce,
    actor: envelope.choice.player,
    prompt: envelope.choice.prompt,
    options: envelope.choice.options,
    context: envelope.choice.context,
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
        pendingChoice: message.pending_choice ? pendingChoice(message.pending_choice) : null,
        events: message.type === "initial_snapshot" ? serverEventLog(message.events) : state.events,
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
                cursor: Math.max(state.history.cursor, message.entry.decision_count),
                redo_count: 0,
              },
      };
    case "pending_choice":
      return {
        ...state,
        gameVersion: message.game_version,
        pendingChoice: pendingChoice({ nonce: message.nonce, choice: message.choice }),
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
    case "pong":
      return state;
  }
}

/** Owns every network ingress point and the lifecycle of one game-session connection. */
export class GameSessionClient {
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
  private pingSequence = 0;
  private pendingBatch: { nonce: string; plan: string; requestId: string } | null = null;

  constructor(private readonly options: GameSessionClientOptions) {}

  getState(): GameSessionState {
    return this.state;
  }

  subscribe(listener: Listener): () => void {
    this.listeners.add(listener);
    return () => this.listeners.delete(listener);
  }

  start(): void {
    this.stopped = false;
    this.setState({ ...this.state, status: "connecting", lastError: null });
    void this.loadSnapshot();
    this.openSocket();
  }

  stop(): void {
    this.stopped = true;
    this.clearTimers();
    this.detachSocket();
    this.rejectSubmission("Submission stopped");
  }

  async submitChoice(optionId: string): Promise<void> {
    const { pendingChoice, gameVersion } = this.state;
    if (!this.socket || this.socket.readyState !== WebSocket.OPEN) {
      const message = "Cannot submit choice: not connected to server";
      this.setState({ ...this.state, lastError: message });
      throw new Error(message);
    }
    if (!pendingChoice) {
      const message = "No decision currently pending";
      this.setState({ ...this.state, lastError: message });
      throw new Error(message);
    }
    if (this.submission) {
      if (this.submission.nonce === pendingChoice.nonce && this.submission.optionId === optionId)
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
    const promise = new Promise<void>((done, fail) => {
      resolve = done;
      reject = fail;
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
    return promise;
  }

  async submitMovementBatch(destination: string, steps: MovementStep[]): Promise<void> {
    return this.submitBatch({ kind: "tactical_movement", destination, steps });
  }

  async submitBatch(
    plan: BasketPlan | { kind: "tactical_movement"; destination: string; steps: MovementStep[] },
  ): Promise<void> {
    if (this.options.viewer.role !== "player" || !this.options.viewer.playerSession)
      throw new Error("A player session is required");
    const pending = this.state.pendingChoice;
    if (!pending || pending.actor !== this.options.viewer.seat || !pending.context)
      throw new Error("Decision is no longer pending");
    const expected = {
      tactical_movement: ["movement_step"],
      payment: ["pay_resources", "pay_influence"],
      agenda_vote_planets: ["vote_exhaust_planet"],
      production: ["produce_unit"],
    }[plan.kind];
    if (!expected.includes(pending.context.subtype))
      throw new Error("Workflow is no longer pending");
    const serialized = JSON.stringify(plan);
    if (this.pendingBatch?.nonce !== pending.nonce || this.pendingBatch.plan !== serialized)
      this.pendingBatch = {
        nonce: pending.nonce,
        plan: serialized,
        requestId: crypto.randomUUID(),
      };
    const response = await fetch(this.snapshotUrl().replace(/\/snapshot$/, "/batches"), {
      method: "POST",
      headers: { ...this.snapshotHeaders(), "content-type": "application/json" },
      body: JSON.stringify({
        request_id: this.pendingBatch.requestId,
        expected_version: this.state.gameVersion,
        nonce: pending.nonce,
        plan,
      }),
    });
    if (!response.ok) {
      if (response.status !== 500 && response.status !== 502 && response.status !== 503)
        this.pendingBatch = null;
      const failure = (await response.json()) as {
        failed_step?: number;
        reason?: string;
        expected?: string;
      };
      throw new Error(
        `Batch step ${(failure.failed_step ?? 0) + 1}: ${failure.reason ?? "batch rejected"}${failure.expected ? ` (${failure.expected})` : ""}`,
      );
    }
    this.pendingBatch = null;
    const result = (await response.json()) as { snapshot: unknown; active?: boolean };
    if (result.active === false)
      throw new Error(
        "This confirmation was already committed but is now undone. Refresh the decision before confirming again.",
      );
    const snapshot = decodeInitialSnapshot(
      { type: "initial_snapshot", ...(result.snapshot as object) },
      this.options.gameId,
    );
    this.rejectSubmission("Game history changed");
    this.detachSocket();
    this.clearTimers();
    this.setState(
      reduceServerMessage(
        { ...this.state, pendingChoice: null, lastError: null },
        { ...snapshot, type: "initial_snapshot" },
      ),
    );
    this.openSocket();
  }

  /** The host changes the authoritative Rust timeline; all clients reconnect to it. */
  async changeHistory(action: HistoryChange): Promise<void> {
    if (this.options.viewer.role !== "player" || !this.options.viewer.playerSession)
      throw new Error("A player session is required");
    const url = this.snapshotUrl().replace(/\/snapshot$/, "/history");
    const body =
      typeof action === "string"
        ? { action }
        : "cursor" in action
          ? { action: "restore_cursor", cursor: action.cursor }
          : { action: "restore", event_id: action.eventId };
    let version = this.state.gameVersion;
    const cursor = this.state.history.cursor;
    let response!: Response;
    let conflictReason: string | undefined;
    for (let attempt = 0; attempt < HISTORY_RETRY_ATTEMPTS; attempt++) {
      conflictReason = undefined;
      response = await fetch(url, {
        method: "POST",
        headers: { ...this.snapshotHeaders(), "content-type": "application/json" },
        body: JSON.stringify({ ...body, expected_version: version }),
      });
      if (response.ok || response.status !== 409) break;
      conflictReason = await response.text();
      if (
        attempt === HISTORY_RETRY_ATTEMPTS - 1 ||
        !conflictReason.includes("Game advanced or a decision is in flight")
      )
        break;
      // The worker may still be advancing automatically toward its next human choice.
      // Refresh the version, but never rewind a different decision if someone acted meanwhile.
      await new Promise((resolve) => setTimeout(resolve, 100));
      const latest = await fetch(this.snapshotUrl(), { headers: this.snapshotHeaders() });
      if (!latest.ok) break;
      const snapshot = decodeInitialSnapshot(await latest.json(), this.options.gameId);
      if (snapshot.history?.cursor !== cursor) break;
      version = snapshot.game_version;
    }
    if (!response.ok) {
      const reason = conflictReason ?? (await response.text());
      const error = `History change failed (${response.status}): ${reason}`;
      this.setState({ ...this.state, lastError: error });
      throw new Error(error);
    }
    const snapshot = decodeInitialSnapshot(await response.json(), this.options.gameId);
    const expected = this.options.viewer;
    if (
      snapshot.viewer.role !== expected.role ||
      (expected.role === "player" &&
        (snapshot.viewer.role !== "player" || snapshot.viewer.seat !== expected.seat))
    ) {
      throw new Error("Server viewer identity does not match this session");
    }
    this.rejectSubmission("Game history changed");
    this.detachSocket();
    this.clearTimers();
    this.setState(
      reduceServerMessage(
        { ...this.state, pendingChoice: null, lastError: null },
        { ...snapshot, type: "initial_snapshot" },
      ),
    );
    this.openSocket();
  }

  private async loadSnapshot(): Promise<void> {
    try {
      const response = await fetch(this.snapshotUrl(), { headers: this.snapshotHeaders() });
      if (!response.ok) throw new Error(`Snapshot request failed (${response.status})`);
      this.ingestHttpSnapshot(await response.json());
    } catch (error) {
      if (!this.stopped)
        this.setState({ ...this.state, lastError: `Snapshot request failed: ${String(error)}` });
    }
  }

  private openSocket(): void {
    const socket = new WebSocket(this.webSocketUrl());
    this.socket = socket;
    socket.onopen = () => {
      if (this.stopped || this.socket !== socket) return;
      const playerSession =
        this.options.viewer.role === "player" ? this.options.viewer.playerSession : undefined;
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
      this.setState({ ...this.state, status: "connected", lastError: null });
    };
    socket.onmessage = (event) => this.ingestWebSocket(event.data);
    socket.onerror = () => {
      if (!this.stopped && this.socket === socket) {
        this.setState({
          ...this.state,
          status: "error",
          lastError: "WebSocket network error occurred",
        });
      }
    };
    socket.onclose = (event) => {
      if (!this.stopped && this.socket === socket) {
        this.clearTimers();
        this.socket = null;
        this.rejectSubmission("Submission disconnected before confirmation");
        this.setState({
          ...this.state,
          status: "disconnected",
          pendingChoice: null,
          snapshot: null,
        });
        this.retry = setTimeout(
          () => {
            if (!this.stopped) {
              void this.loadSnapshot();
              this.openSocket();
            }
          },
          event?.code === 4001 ? 0 : 2_000,
        );
      }
    };
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
      this.apply(
        decodeServerMessage(
          typeof value === "string" ? JSON.parse(value) : value,
          this.options.gameId,
        ),
      );
    } catch (error) {
      if (!this.stopped) {
        const message = `Invalid server message: ${String(error)}`;
        this.setState({ ...this.state, lastError: message });
        this.rejectSubmission(message);
      }
    }
  }

  private apply(message: ServerMessage): void {
    if (message.type === "initial_snapshot" || message.type === "state_update") {
      const expected = this.options.viewer;
      if (
        message.viewer.role !== expected.role ||
        (expected.role === "player" &&
          (message.viewer.role !== "player" || message.viewer.seat !== expected.seat))
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
          pending.optionId === message.option_id && message.game_version >= pending.version,
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
    for (const prior of this.priorSubmissions.splice(0)) prior.reject(new Error(reason));
  }

  private detachSocket(): void {
    const socket = this.socket;
    this.socket = null;
    if (!socket) return;
    socket.onopen = null;
    socket.onmessage = null;
    socket.onerror = null;
    socket.onclose = null;
    if (socket.readyState === WebSocket.CONNECTING || socket.readyState === WebSocket.OPEN)
      socket.close();
  }

  private clearTimers(): void {
    if (this.heartbeat) clearInterval(this.heartbeat);
    if (this.retry) clearTimeout(this.retry);
    this.heartbeat = null;
    this.retry = null;
  }

  private setState(next: GameSessionState): void {
    this.state = next;
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
    return this.options.viewer.role === "player" && this.options.viewer.playerSession
      ? { "x-ti4-player-session": this.options.viewer.playerSession }
      : {};
  }
}
