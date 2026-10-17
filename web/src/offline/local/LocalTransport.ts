import {
  canContinueBatch,
  checkChoiceNonce,
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
import { decodeServerMessage } from "@ti4/shared/wire/decode.ts";
import {
  MessageDrivenState,
  type GameTransport,
  type TransportCapabilities,
} from "../../protocol/transport.ts";
import { PROTOCOL_VERSION, type ReactionModeSetting, type ServerMessage } from "../../protocol/types.ts";
import type { TurnRedoCommand, TurnRedoStatus } from "../../protocol/turnRedo.ts";
import type { Envelope, HostProgress, HostSave, StartConfig } from "@ti4/shared/engine/host.ts";
import type { HostPort } from "@ti4/shared/engine/protocol.ts";
import { engineLink, type EngineLink } from "@ti4/shared/link/engineLink.ts";
import type { HostCommand, LinkEvent } from "@ti4/shared/link/link.ts";

/** What a local game offers in `GameTransport` terms. The shell's `Capabilities` say the same. */
export const LOCAL_TRANSPORT_CAPABILITIES: TransportCapabilities = {
  splicePreview: false,
  turnRedo: true,
  multiplayer: false,
  feedback: true,
};

/** Server messages of protocols this client does not use (see GameSessionClient). */
const IGNORED = new Set(["draft_outcome", "draft_applied"]);

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

/**
 * A game session against the engine in a Web Worker instead of a server. Commands go to the
 * worker over an `engineLink`, the `ServerMessage` objects it tells go through the same decoder
 * and reducer as the server's (`MessageDrivenState`), so the UI cannot tell the difference. One
 * human; the other seats are rule bots that play inside the engine.
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
    history: { cursor: 0, redo_count: 0, generation: 0 },
  });
  private link: EngineLink | null = null;
  /** The link ended because of the saved game (it does not play back, or it was deleted), not the engine. */
  private gone = false;
  private nextRequestId = 1;
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

  /** The faction of each seat (seat `a` first), as the board shows it; null where it is not known. */
  factions(players: number): Array<string | null> {
    const view = this.store.get().snapshot?.view;
    return Array.from({ length: players }, (_, place) => {
      const faction = view?.players.find((p) => p.id === String.fromCharCode(97 + place))?.faction;
      return typeof faction === "string" ? faction : null;
    });
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
    this.link?.close();
    this.link = null;
    this.gone = false;
    try {
      const port = await this.options.createPort();
      if (this.closed) {
        port.terminate();
        return;
      }
      const link = engineLink(port, config, { gate: (envelope) => this.gate(envelope) });
      this.link = link;
      link.subscribe((event) => {
        if (link === this.link) this.receive(event);
      });
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
    this.link?.close();
    this.link = null;
  }

  /** The Retry button after the engine stopped: play the stored game again in a new engine. */
  retryNow(): void {
    if (this.closed) return;
    this.store.set({ ...this.store.get(), status: "connecting", fatal: null, lastError: null });
    const save = this.latest;
    void this.boot(save ? { resume: save } : this.options.config);
  }

  /** The latest save (the answers so far); null before the first question. */
  latestSave(): HostSave | null {
    return this.latest;
  }

  // --- the engine's messages ---------------------------------------------------------------

  private receive(event: LinkEvent): void {
    switch (event.kind) {
      case "message":
        this.ingest(event.message);
        return;
      case "save":
        this.remember(event.save);
        return;
      case "progress":
        this.options.onProgress?.({ done: event.done, total: event.total });
        return;
      case "opened":
        this.store.set({ ...this.store.get(), status: "connected", fatal: null, reconnecting: false });
        this.readyHandlers?.resolve();
        this.readyHandlers = null;
        return;
      case "fatal":
        if (this.gone) this.fail(event.message, "gone");
        else this.fail(`The game engine stopped: ${event.message}`);
        return;
      case "offline":
        return;
    }
  }

  /**
   * Holds the first question of a saved game that did not play back to where it was, until the
   * person decides; a refusal ends the link and the game is "gone".
   */
  private gate(envelope: Envelope): void | Promise<void> {
    const check = envelope.resume_check;
    if (!check || check.ok || this.readOnly) return;
    const decide = this.options.onDiverged;
    if (!decide) {
      this.gone = true;
      throw new Error(
        "This saved game does not play back to where it was. The game engine has changed since it was saved.",
      );
    }
    return decide(check)
      .then((choice) => {
        if (choice === "delete") {
          this.deletedOnRequest = true;
          throw new Error("The game was deleted.");
        }
        if (choice === "view-only") this.readOnly = true;
      })
      .catch((error: unknown) => {
        this.gone = true;
        throw error;
      });
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
  private ingest(value: unknown): void {
    const type = (value as { type?: unknown } | null)?.type;
    if (typeof type === "string" && IGNORED.has(type)) return;
    try {
      const message = decodeServerMessage(this.relabel(value), this.options.gameId);
      if (message.type !== "reach_into" && message.type !== "secondary_preview") this.store.ingest(message);
    } catch (error) {
      this.store.set({ ...this.store.get(), lastError: `Invalid engine message: ${String(error)}` });
    }
  }

  private fail(message: string, kind: "gone" | "unreachable" = "unreachable"): void {
    this.store.set({
      ...this.store.get(),
      status: "error",
      lastError: message,
      fatal: { kind, message },
    });
    this.readyHandlers?.reject(new Error(message));
    this.readyHandlers = null;
  }

  // --- commands ------------------------------------------------------------------------------

  /** A question of the engine of this game (the odds of a combat); it runs in the game's Worker. */
  queryEngine(request: object): Promise<unknown> {
    const link = this.link;
    if (!link) return Promise.reject(new Error(NOT_RUNNING));
    return link.query(request);
  }

  /** A command to the engine; the messages of its reply reach the reducer before it resolves. */
  private send(request: HostCommand): Promise<Envelope> {
    const link = this.link;
    if (!link) return Promise.reject(new Error(NOT_RUNNING));
    return link.request(request);
  }

  private message<T extends object>(fields: T): T & { protocol_version: number; game_id: string } {
    return { protocol_version: PROTOCOL_VERSION, game_id: this.options.gameId, ...fields };
  }

  /** Resolves with the envelope of the next question (or end) the engine reaches. */
  private nextQuestion(): { promise: Promise<Envelope | null>; cancel: () => void } {
    if (!this.link) throw new Error(NOT_RUNNING);
    const next = this.link.nextQuestion();
    // A failed command is thrown first; the question it then no longer waits for fails with it.
    next.promise.catch(() => undefined);
    return next;
  }

  private setError(message: string): never {
    this.store.set({ ...this.store.get(), lastError: message });
    throw new Error(message);
  }

  async submitChoice(optionId: string, nonce?: string): Promise<void> {
    if (this.readOnly) return this.setError(VIEW_ONLY);
    const { pendingChoice, gameVersion } = this.store.get();
    checkChoiceNonce(pendingChoice, nonce);
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
        if (reply.resume === undefined) {
          next.cancel();
          throw new Error(batchFailure(reply.batch?.message));
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
    void this.send(this.message({ type: "set_reaction_mode", card, mode })).catch((error: unknown) =>
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
    await this.sendHistory(wire);
  }

  private async sendHistory(wire: unknown): Promise<void> {
    await this.stored();
    const next = this.nextQuestion();
    try {
      const reply = await this.send({ type: "history", action: wire });
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
    const requestId = this.nextRequestId++;
    let reply: Envelope;
    try {
      reply = await this.send(this.message({ type: "query_reach", request_id: requestId, system_id: systemId }));
    } catch (error) {
      throw error instanceof Error ? error : new Error(String(error));
    }
    const found = this.decodeAll(reply.messages).find((m) => m.type === "reach_into");
    if (found?.type === "reach_into") {
      return found.refused
        ? { kind: "refused", reason: found.refused }
        : { kind: "reach", systemId: found.system_id, asOfVersion: found.as_of_version, origins: found.origins };
    }
    throw new ReachUnsupportedError("The engine could not answer who can go there");
  }

  /** Offline the page uses its own estimate: this transport does not ask the host (`preview_secondary`). */
  async previewSecondary(
    _card: string,
    _primary: string,
    _answers: readonly string[],
  ): Promise<SecondaryPreviewReply> {
    throw new PreviewUnsupportedError("The offline engine cannot preview a secondary");
  }

  /**
   * The engine names the game as it was set up (`"local"`, or the id of the page that started it);
   * the store's id names it here. An imported game and a game of web2 have another id than their
   * engine's, so the messages take the store's id before they are decoded.
   */
  private relabel(value: unknown): unknown {
    if (value === null || typeof value !== "object" || !("game_id" in value)) return value;
    return { ...value, game_id: this.options.gameId };
  }

  /** The messages of a reply, decoded again (the reducer has had them, and any error in them). */
  private decodeAll(raw: readonly unknown[]): ServerMessage[] {
    const decoded: ServerMessage[] = [];
    for (const value of raw) {
      try {
        decoded.push(decodeServerMessage(this.relabel(value), this.options.gameId));
      } catch {
        // Told as lastError when the reply came in.
      }
    }
    return decoded;
  }

  async fetchTurnRedoStatus(): Promise<TurnRedoStatus | null> {
    return null;
  }

  /**
   * "Undo my last turn": the engine takes the game back to the start of the person's last turn(s)
   * (a restart from the answers before it, like any history change). There is no auto-play of
   * recorded decisions and no saved original: the bots just decide again from the same game, and
   * the undone answers can be redone with the history's redo. `autoplay`, `restore` and `keep`
   * have nothing to do (a redo is never "in flight" offline).
   */
  async turnRedoCommand(command: TurnRedoCommand): Promise<void> {
    if (command.action !== "request") return;
    await this.sendHistory({ undo_turn: { turns: command.turns === 2 ? 2 : 1 } });
  }

  private seat(): string | null {
    const snapshot = this.store.get().snapshot;
    return snapshot?.viewer.role === "player" ? snapshot.viewer.seat : null;
  }
}

const NOT_RUNNING = "The game engine is not running";

const VIEW_ONLY = "This game is open to look at only (it was saved with another version of the engine).";

const batchFailure = (message: string | undefined): string =>
  message ? `Batch rejected: ${message}` : "Batch rejected";
