import {
  canContinueBatch,
  PLAN_SUBTYPES,
  PreviewUnsupportedError,
  ReachUnsupportedError,
  type BatchPlan,
  type GameSessionState,
  type HistoryChange,
  type MovementStep,
  type ReachReply,
  type SecondaryPreviewReply,
} from "../../protocol/client.ts";
import { decodeServerMessage } from "../../protocol/decode.ts";
import {
  MessageDrivenState,
  type GameTransport,
  type TransportCapabilities,
} from "../../protocol/transport.ts";
import { PROTOCOL_VERSION, type ReactionModeSetting, type ServerMessage } from "../../protocol/types.ts";
import type { TurnRedoCommand, TurnRedoStatus } from "../../protocol/turnRedo.ts";
import type { Envelope, HostProgress, HostSave, StartConfig } from "./hostApi.ts";
import type { FromWorker, HostPort } from "./hostProtocol.ts";

/** What a local game offers in `GameTransport` terms. The shell's `Capabilities` say the same. */
export const LOCAL_TRANSPORT_CAPABILITIES: TransportCapabilities = {
  splicePreview: false,
  turnRedo: false,
  advisor: false,
  multiplayer: false,
  feedback: true,
};

/** Server messages of protocols this client does not use (see GameSessionClient). */
const IGNORED = new Set(["planning_status", "planning_update", "planning_result", "draft_outcome", "draft_applied"]);

/** After the page stops using the transport, the engine is shut down this much later (React remounts in dev). */
const CLOSE_DELAY_MS = 1_500;

export interface LocalTransportOptions {
  gameId: string;
  /** Hash in the name of the engine file this game runs on. */
  engineHash?: string;
  /** sha-256 of the engine file itself: what feedback and the saved game name. */
  wasmSha256?: string;
  /** Open to look at only: every command that would change the game is refused. */
  viewOnly?: boolean;
  /**
   * The replay of a saved game gave other decisions than the save had. The answer says what to
   * do (the page asks the person); without this the run fails as "gone".
   */
  onDiverged?: (check: { ok: boolean; [key: string]: unknown }) => Promise<"view-only" | "try-anyway" | "delete">;
  /** The player's name; stays on this device (the engine never sees it). */
  nickname?: string;
  /** A port to an engine that has its module (a Worker after `init`). Called again to recover. */
  createPort: () => Promise<HostPort>;
  /** How the first run starts: a new game, or a save to play again. */
  config: StartConfig;
  /**
   * The game as it stands at each question (and when it ends): the launcher stores it. The
   * promise settles when the save is durable; the next command of the person (an answer, a
   * batch, a history change) waits for it, so an answer is never given on top of an unsaved
   * position.
   */
  onSave?: (save: HostSave) => void | Promise<void>;
  /** Replay progress, while a saved game (or a rewind) is played again. */
  onProgress?: (progress: HostProgress) => void;
}

type Waiter = {
  resolve: (envelope: Envelope | null) => void;
  reject: (error: Error) => void;
};

/**
 * A game session against the engine in a Web Worker instead of a server. Commands go to the
 * worker, the `ServerMessage` objects it returns go through the same decoder and reducer as the
 * server's (`MessageDrivenState`), so the UI cannot tell the difference. One human; the other
 * seats are rule bots that play inside the engine.
 */
export class LocalTransport implements GameTransport {
  readonly capabilities = LOCAL_TRANSPORT_CAPABILITIES;

  private readonly store = new MessageDrivenState({
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
  });
  private port: HostPort | null = null;
  private nextId = 1;
  private readonly replies = new Map<number, { resolve: (e: Envelope) => void; reject: (e: Error) => void }>();
  private waiters: Waiter[] = [];
  private inflight: { nonce: string; optionId: string; promise: Promise<void> } | null = null;
  private batching: { nonce: string; plan: string; promise: Promise<void> } | null = null;
  private latest: HostSave | null = null;
  private closeTimer: ReturnType<typeof setTimeout> | null = null;
  private closed = false;
  private saving: Promise<void> = Promise.resolve();
  private readOnly: boolean;
  /** Set when the person chose to delete the game while it was being opened. */
  deletedOnRequest = false;
  private readyHandlers: { resolve: () => void; reject: (error: Error) => void } | null = null;
  /** Resolves with the first question; rejects if the run cannot start (or the save cannot be played). */
  readonly ready: Promise<void>;

  constructor(private readonly options: LocalTransportOptions) {
    this.readOnly = options.viewOnly === true;
    this.ready = new Promise<void>((resolve, reject) => {
      this.readyHandlers = { resolve, reject };
    });
    // Nobody may be waiting on it (a game opened by the launcher always is, tests may not be).
    this.ready.catch(() => undefined);
  }

  get engineHash(): string {
    return this.options.engineHash ?? "dev";
  }

  get wasmSha256(): string {
    return this.options.wasmSha256 ?? "unknown";
  }

  /** True while the game is open to look at only. */
  /** The faction of the human, as the board shows it; null before the first question. */
  humanFaction(): string | null {
    const snapshot = this.store.get().snapshot;
    if (snapshot?.viewer.role !== "player") return null;
    const seat = snapshot.viewer.seat;
    const player = snapshot.view.players.find((p) => p.id === seat);
    return typeof player?.faction === "string" ? player.faction : null;
  }

  get viewOnly(): boolean {
    return this.readOnly;
  }

  get nickname(): string {
    return this.options.nickname ?? "You";
  }

  /** Starts the engine (the launcher calls this; `start()` of the hook only reattaches). */
  async begin(): Promise<void> {
    await this.boot(this.options.config);
  }

  private async boot(config: StartConfig): Promise<void> {
    this.port?.terminate();
    this.port = null;
    try {
      const port = await this.options.createPort();
      if (this.closed) {
        port.terminate();
        return;
      }
      this.port = port;
      port.listen((message) => this.receive(port, message));
      port.post({ t: "start", config });
    } catch (error) {
      this.fail(error instanceof Error ? error.message : String(error));
    }
  }

  // --- GameTransport: lifecycle ------------------------------------------------------------

  getState(): GameSessionState {
    return this.store.get();
  }

  subscribe(listener: () => void): () => void {
    return this.store.subscribe(listener);
  }

  start(): void {
    if (this.closeTimer) clearTimeout(this.closeTimer);
    this.closeTimer = null;
  }

  stop(): void {
    if (this.closeTimer) clearTimeout(this.closeTimer);
    this.closeTimer = setTimeout(() => this.close(), CLOSE_DELAY_MS);
  }

  /** Shuts the engine down for good. */
  close(): void {
    this.closed = true;
    this.port?.terminate();
    this.port = null;
    this.settle(new Error("The game was closed"));
  }

  /** The Retry button after the engine stopped: play the stored game again in a new engine. */
  retryNow(): void {
    if (this.closed) return;
    this.store.set({ ...this.store.get(), status: "connecting", fatal: null, lastError: null });
    const save = this.latest;
    void this.boot(save ? { setup: save.setup, resume: save } : this.options.config);
  }

  /** The latest save (the answers so far); null before the first question. */
  latestSave(): HostSave | null {
    return this.latest;
  }

  // --- the engine's messages ---------------------------------------------------------------

  private receive(port: HostPort, message: FromWorker): void {
    if (port !== this.port) return;
    switch (message.t) {
      case "ready":
        return;
      case "fatal":
      case "crash":
        this.fail(message.message);
        return;
      case "progress":
        this.options.onProgress?.(message.progress);
        return;
      case "reply": {
        const pending = this.replies.get(message.id);
        this.replies.delete(message.id);
        if (!pending) return;
        if (message.error !== undefined) pending.reject(new Error(message.error));
        else pending.resolve(message.envelope ?? ({ kind: "command", messages: [] } as Envelope));
        return;
      }
      case "ask":
        this.onAsk(message.envelope, message.save);
        return;
      case "end":
        this.onEnd(message.envelope, message.save);
    }
  }

  private onAsk(envelope: Envelope, save: HostSave | null): void {
    const check = envelope.resume_check;
    if (check && !check.ok && !this.readOnly) {
      const decide = this.options.onDiverged;
      if (!decide) {
        this.fail(
          "This saved game does not play back to where it was. The game engine has changed since it was saved.",
          "gone",
        );
        return;
      }
      void decide(check).then(
        (choice) => {
          if (this.closed) return;
          if (choice === "delete") {
            this.deletedOnRequest = true;
            this.fail("The game was deleted.", "gone");
            return;
          }
          if (choice === "view-only") this.readOnly = true;
          this.accept(envelope, save);
        },
        (error: unknown) => this.fail(error instanceof Error ? error.message : String(error), "gone"),
      );
      return;
    }
    this.accept(envelope, save);
  }

  private accept(envelope: Envelope, save: HostSave | null): void {
    // The messages first: the stored game names the human's faction, which the board tells.
    this.ingest(envelope.messages);
    this.remember(save);
    this.store.set({ ...this.store.get(), status: "connected", fatal: null, reconnecting: false });
    this.readyHandlers?.resolve();
    this.readyHandlers = null;
    this.settle(null, envelope);
  }

  private onEnd(envelope: Envelope, save: HostSave | null): void {
    this.remember(save);
    this.ingest(envelope.messages);
    const error = envelope.end?.error;
    if (error) {
      this.fail(`The game engine stopped: ${error}`);
      return;
    }
    this.store.set({ ...this.store.get(), status: "connected" });
    this.readyHandlers?.resolve();
    this.readyHandlers = null;
    this.settle(null, envelope);
  }

  private remember(save: HostSave | null): void {
    if (!save) return;
    this.latest = save;
    if (this.readOnly || !this.options.onSave) return;
    // Saves are written one after the other, newest last; a failing store never stops the game.
    const previous = this.saving;
    this.saving = previous
      .then(() => this.options.onSave?.(save))
      .catch(() => undefined);
  }

  /** The write-ahead: waits until the position the person is looking at is stored. */
  private async stored(): Promise<void> {
    if (this.readOnly) throw new Error(VIEW_ONLY);
    await this.saving;
  }

  /** Decodes like the WebSocket path of GameSessionClient and feeds the shared reducer. */
  private ingest(raw: readonly unknown[]): ServerMessage[] {
    const accepted: ServerMessage[] = [];
    for (const value of raw) {
      const type = (value as { type?: unknown } | null)?.type;
      if (typeof type === "string" && IGNORED.has(type)) continue;
      try {
        const message = decodeServerMessage(value, this.options.gameId);
        accepted.push(message);
        if (message.type !== "reach_into" && message.type !== "secondary_preview")
          this.store.ingest(message);
      } catch (error) {
        this.store.set({ ...this.store.get(), lastError: `Invalid engine message: ${String(error)}` });
      }
    }
    return accepted;
  }

  private fail(message: string, kind: "gone" | "unreachable" = "unreachable"): void {
    this.store.set({
      ...this.store.get(),
      status: "error",
      lastError: message,
      fatal: { kind, message },
    });
    const error = new Error(message);
    this.readyHandlers?.reject(error);
    this.readyHandlers = null;
    this.settle(error);
    for (const pending of this.replies.values()) pending.reject(error);
    this.replies.clear();
  }

  // --- commands ------------------------------------------------------------------------------

  private send(request: object): Promise<Envelope> {
    const port = this.port;
    if (!port) return Promise.reject(new Error("The game engine is not running"));
    const id = this.nextId++;
    return new Promise<Envelope>((resolve, reject) => {
      this.replies.set(id, { resolve, reject });
      port.post({ t: "command", id, request });
    });
  }

  private message<T extends object>(fields: T): T & { protocol_version: number; game_id: string } {
    return { protocol_version: PROTOCOL_VERSION, game_id: this.options.gameId, ...fields };
  }

  /** Resolves with the envelope of the next question (or end) the engine reaches. */
  private nextQuestion(): { promise: Promise<Envelope | null>; cancel: () => void } {
    let waiter!: Waiter;
    const promise = new Promise<Envelope | null>((resolve, reject) => {
      waiter = { resolve, reject };
      this.waiters.push(waiter);
    });
    return { promise, cancel: () => (this.waiters = this.waiters.filter((w) => w !== waiter)) };
  }

  private settle(error: Error | null, envelope: Envelope | null = null): void {
    const waiting = this.waiters;
    this.waiters = [];
    for (const waiter of waiting) {
      if (error) waiter.reject(error);
      else waiter.resolve(envelope);
    }
  }

  private setError(message: string): never {
    this.store.set({ ...this.store.get(), lastError: message });
    throw new Error(message);
  }

  async submitChoice(optionId: string): Promise<void> {
    if (this.readOnly) return this.setError(VIEW_ONLY);
    const { pendingChoice, gameVersion } = this.store.get();
    if (!pendingChoice) return this.setError("No decision currently pending");
    if (this.inflight) {
      if (this.inflight.nonce === pendingChoice.nonce) {
        if (this.inflight.optionId === optionId) return this.inflight.promise;
        throw new Error("Another choice submission is still pending");
      }
      this.inflight = null;
    }
    const entry = { nonce: pendingChoice.nonce, optionId, promise: Promise.resolve() };
    entry.promise = (async () => {
      await this.stored();
      const next = this.nextQuestion();
      try {
        const reply = await this.send(
          this.message({
            type: "submit_choice",
            nonce: pendingChoice.nonce,
            expected_version: gameVersion,
            option_id: optionId,
          }),
        );
        this.ingest(reply.messages);
        if (reply.resume === undefined) {
          next.cancel();
          throw new Error(this.store.get().lastError ?? "The choice was not accepted");
        }
        await next.promise;
      } catch (error) {
        next.cancel();
        throw error;
      } finally {
        if (this.inflight === entry) this.inflight = null;
      }
    })();
    this.inflight = entry;
    return entry.promise;
  }

  async submitMovementBatch(destination: string, steps: MovementStep[]): Promise<void> {
    return this.submitBatch({ kind: "tactical_movement", destination, steps });
  }

  async submitBatch(plan: BatchPlan): Promise<void> {
    if (this.readOnly) throw new Error(VIEW_ONLY);
    const pending = this.store.get().pendingChoice;
    const seat = this.seat();
    if (!pending || pending.actor !== seat || !pending.context) throw new Error("Decision is no longer pending");
    if (!PLAN_SUBTYPES[plan.kind].includes(pending.context.subtype))
      throw new Error("Workflow is no longer pending");
    const serialized = JSON.stringify(plan);
    if (this.batching && this.batching.nonce === pending.nonce) {
      if (this.batching.plan === serialized) return this.batching.promise;
      throw new Error("Another confirmation for this decision is still being sent");
    }
    const entry = { nonce: pending.nonce, plan: serialized, promise: Promise.resolve() };
    entry.promise = (async () => {
      await this.stored();
      const next = this.nextQuestion();
      try {
        const reply = await this.send({
          type: "submit_batch",
          nonce: pending.nonce,
          expected_version: this.store.get().gameVersion,
          plan,
        });
        this.ingest(reply.messages);
        if (reply.resume === undefined) {
          next.cancel();
          throw new Error(batchFailure(reply.batch?.message) );
        }
        const envelope = await next.promise;
        this.finishBatch(plan, envelope?.batch);
      } catch (error) {
        next.cancel();
        throw error;
      } finally {
        if (this.batching === entry) this.batching = null;
      }
    })();
    this.batching = entry;
    return entry.promise;
  }

  private finishBatch(plan: BatchPlan, batch: Envelope["batch"]): void {
    if (!batch) return;
    if (batch.status === "refused") throw new Error(batchFailure(batch.message));
    const state = this.store.get();
    if (batch.status === "interrupted" && batch.remaining_steps?.length) {
      this.store.set({
        ...state,
        batchResume: {
          plan: { ...plan, steps: batch.remaining_steps } as BatchPlan,
          applied: batch.applied,
          waiting: { subtype: batch.waiting?.subtype ?? null, ownSeat: batch.waiting?.own_seat ?? false },
        },
      });
    } else if (state.batchResume) this.store.set({ ...state, batchResume: null });
  }

  async resumeBatch(): Promise<void> {
    const state = this.store.get();
    const resume = state.batchResume;
    if (!resume) throw new Error("There is no paused plan to continue");
    if (!canContinueBatch(resume, state.pendingChoice, this.seat()))
      throw new Error("Workflow is no longer pending");
    try {
      await this.submitBatch(resume.plan);
    } catch (error) {
      if (this.store.get().batchResume === resume)
        this.store.set({
          ...this.store.get(),
          batchResume: null,
          lastError: error instanceof Error ? error.message : String(error),
        });
      throw error;
    }
  }

  dismissBatchResume(): void {
    const state = this.store.get();
    if (state.batchResume) this.store.set({ ...state, batchResume: null });
  }

  setReactionMode(card: string, mode: ReactionModeSetting): void {
    if (this.readOnly) return;
    void this.send(this.message({ type: "set_reaction_mode", card, mode }))
      .then((reply) => this.ingest(reply.messages))
      .catch((error: unknown) =>
        this.store.set({
          ...this.store.get(),
          lastError: `Could not change the setting: ${error instanceof Error ? error.message : String(error)}`,
        }),
      );
  }

  /** Bluff holds exist only against people; the engine ignores them. */
  setReactionIntent(): void {}
  passReactionHold(): void {}

  async changeHistory(action: HistoryChange): Promise<void> {
    const wire =
      typeof action === "string"
        ? action
        : "cursor" in action
          ? { restore_cursor: { cursor: action.cursor } }
          : { restore: { event_id: action.eventId } };
    await this.stored();
    const next = this.nextQuestion();
    try {
      const reply = await this.send({ type: "history", action: wire });
      this.ingest(reply.messages);
      if (reply.resume === undefined) {
        next.cancel();
        const message = this.store.get().lastError ?? "That history change is not possible";
        throw new Error(message);
      }
      // An undo goes on at once; a redo or rewind plays the game again in a new engine.
      await next.promise;
      this.store.set({ ...this.store.get(), batchResume: null, lastError: null });
    } catch (error) {
      next.cancel();
      throw error;
    }
  }

  async fetchReplay(): Promise<{ text: string; filename: string }> {
    const save = this.latest;
    if (!save) throw new Error("The game has not started");
    return { text: JSON.stringify(save, null, 2), filename: `ti4-offline-${this.options.gameId}.json` };
  }

  async queryReach(systemId: string): Promise<ReachReply> {
    const requestId = this.nextId++;
    let reply: Envelope;
    try {
      reply = await this.send(this.message({ type: "query_reach", request_id: requestId, system_id: systemId }));
    } catch (error) {
      throw error instanceof Error ? error : new Error(String(error));
    }
    const answers = this.ingestAnswer(reply);
    const found = answers.find((m) => m.type === "reach_into");
    if (found?.type === "reach_into") {
      return found.refused
        ? { kind: "refused", reason: found.refused }
        : { kind: "reach", systemId: found.system_id, asOfVersion: found.as_of_version, origins: found.origins };
    }
    throw new ReachUnsupportedError("The engine could not answer who can go there");
  }

  /** The hosted game has no secondary preview: the page falls back to its own estimate. */
  async previewSecondary(
    _card: string,
    _primary: string,
    _answers: readonly string[],
  ): Promise<SecondaryPreviewReply> {
    throw new PreviewUnsupportedError("The offline engine cannot preview a secondary");
  }

  private ingestAnswer(reply: Envelope): ServerMessage[] {
    return this.ingest(reply.messages);
  }

  async fetchTurnRedoStatus(): Promise<TurnRedoStatus | null> {
    return null;
  }

  async turnRedoCommand(_command: TurnRedoCommand): Promise<void> {
    throw new Error("Turn redo is not available in an offline game");
  }

  private seat(): string | null {
    const snapshot = this.store.get().snapshot;
    return snapshot?.viewer.role === "player" ? snapshot.viewer.seat : null;
  }
}

const VIEW_ONLY = "This game is open to look at only (it was saved with another version of the engine).";

const batchFailure = (message: string | undefined): string =>
  message ? `Batch rejected: ${message}` : "Batch rejected";
