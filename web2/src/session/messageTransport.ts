// A game at the level of its messages (`GameLink` of `web-shared/src/link/`) as the events of
// the page. The server (`wsTransport.ts`, over `socketLink`) and the engine of a local game
// (`wasmTransport.ts`, over `engineLink`) speak the same protocol: `ClientMessage` in,
// `ServerMessage` out. What only one of them has is in `MessagePlay`.
import type { GameLink, HostCommand } from "@ti4/shared/link/link.ts";
import { type EventLog, NO_EVENTS, eventsOf, withEvent as withPublicEvent } from "./events";
import type { PlanStep } from "./movementPlan";
import { checkBoard, type BoardCheck } from "./history";
import { NO_ROLLS, type RollLog, actionRolls, logOf, withEvent } from "./rolls";
import { type LegacyStaged, seatSync } from "./seatStore";
import { modesOf, passedOf, planStoppedOf, rejection, updateOf } from "./serverMessages";
import type { Capabilities, PlanStopped, Transport, TransportEvent } from "./transport";
import {
  type BattleResponse,
  type Catalog,
  type ClientRequest,
  type DraftStep,
  type FleetCounts,
  type FleetStanding,
  type GroundOddsRequest,
  type GroundOddsResponse,
  type PlanScope,
  PROTOCOL_VERSION,
  type ReactionModes,
  type ServerMessage,
  type SpaceOddsRequest,
  type StateMsg,
} from "./wire";

type ErrorMessage = Extract<ServerMessage, { type: "error" }>;
type Update = Extract<TransportEvent, { kind: "update" }>;

/** What a side of a game adds to its messages. */
export interface MessagePlay {
  gameId: string;
  /** The printed cards of the engine of this page. A snapshot of other cards is refused. */
  catalog: Catalog | null;
  odds(request: SpaceOddsRequest): Promise<BattleResponse>;
  groundOdds(request: GroundOddsRequest): Promise<GroundOddsResponse>;
  fleetStanding(counts: FleetCounts): Promise<FleetStanding>;
  capabilities?: Capabilities;
  /** The side plays a seat, and has staged work. A spectator has none. */
  player: boolean;
  legacyStaged?: LegacyStaged;
  /** The game takes a message now: the socket is open, or the engine waits for the page. */
  ready(): boolean;
  /**
   * The page keeps the modes of the reactions (a local game): a change is told at once, kept
   * with `onModes`, and sent when the game waits (`flushModes`). Without it the game tells them.
   */
  modes?: { initial?: Readonly<ReactionModes>; onModes?(modes: ReactionModes): void };
  /** A note of the game is told once for each window that it passed, not once. */
  passedEach?: boolean;
  /** The seat that the game waits for before its decision, when the page steps it. */
  stepping?(): string | null;
  /** Asks for the game again: an answer was stale, and the update that explains it did not come. */
  resync?(): void;
  /** Why the game cannot be played: the game refused it before its first update. */
  refused?(error: ErrorMessage): string;
  /** What the page says of a link that cannot go on. Without it: the text of the link. */
  fatal?(message: string): string;
  /** Told with each snapshot of the game: the game was reached. */
  snapshot?(): void;
  /** What a refusal of a write of the staged work says. Without it: the message of the game. */
  stagedRefused?: string;
  /** Told after a write of the staged work was sent. */
  afterPut?(): void;
  /** Told after the transport was closed. */
  closed?(): void;
}

/** What the side that made a message transport reads and does besides the page. */
export interface MessageCore {
  transport: Transport;
  /** The game as told last. */
  latest(): (Update & { version: number }) | null;
  /** A script is on its way: its result is not there yet. */
  sending(): boolean;
  /** An undo is on its way: its answer is the game that is played again. */
  undoing(): boolean;
  /** Tells the game as it is now, once more (a change of `stepping`). */
  again(): void;
  /** Sends the modes that changed while the game did not wait. */
  flushModes(): void;
  /** Tells the listeners an event of the side (a replay that starts). */
  tell(event: TransportEvent): void;
  /** Why the game cannot go on: it is said, and nothing is tried after it. */
  fail(message: string): void;
  /** The link lost the game: what was on its way is gone. */
  lost(): void;
}

const REPLAY_SLICE_MS = 50;
/** After a stale answer the update that explains it is on its way. Without it, the game is asked again. */
const RESYNC_MS = 1_000;

const short = (digest: string | undefined) => (digest ? digest.slice(0, 12) : "none");

/**
 * Plays one game over `link`. The first update is the first state that the game tells: the
 * snapshot after the subscription of a socket, or the first question of the engine. A link with
 * replies (`request`, the engine) tells where an answer ends: a draft that it did not play is
 * told as failed there, and a plan that it refused is told there too.
 */
export function messageTransport(link: GameLink, play: MessagePlay): MessageCore {
  const listeners = new Set<(event: TransportEvent) => void>();
  /** The game as the link told it last, and its version. */
  let latest: (Update & { version: number }) | null = null;
  /** Why the game cannot be played. Nothing is tried after it. */
  let failed: string | null = null;
  let offline = false;
  /** The modes of the seat, as the game or the page set them last. */
  let modes: ReactionModes = { ...play.modes?.initial };
  /** The reactions whose mode changed while the game did not wait: sent at the next question. */
  const unsent = new Set<string>();
  let resync: ReturnType<typeof setTimeout> | null = null;
  /** The newest draft that was asked. An answer for another request is late. */
  let asked: { id: string; script: DraftStep[]; scope?: PlanScope } | null = null;
  /** Why the engine did not play the draft that is asked. */
  let refusal: string | null = null;
  let requests = 0;
  /** The script that is being sent: its result is counted in entries, a plan in steps. */
  let sent: { nonce: string; destination: string; lead: number } | null = null;
  /** Told with the next update: where the last plan stopped. */
  let stopped: PlanStopped | null = null;
  /** A refused plan that waits for the end of its reply: an update in the reply tells it. */
  let held: PlanStopped | null = null;
  /**
   * An undo was sent. The server plays the game again without the decision and closes the
   * socket as replaced: that is no loss of the game, and the snapshot after it is the answer.
   */
  let undoing = false;
  /** The notes of the game that were told. A note has an id so that a repeat can be dropped. */
  const noted = new Set<string>();
  /** The public rolls that the events told, for the update of the action that is played now. */
  let rolls: RollLog = NO_ROLLS;
  /** The public events of the game, for the Log. */
  let events: EventLog = NO_EVENTS;
  /** In development: the board of the last update at the end of a step, to check the next. */
  let board: BoardCheck = null;
  /** The last `replaying` that was told, and when. */
  let shown = 0;
  const replies = typeof link.request === "function";

  const say = (event: TransportEvent) => {
    for (const listener of listeners) {
      listener(event);
    }
  };
  /** A message of the protocol. With replies, the reply; it is told before it settles. */
  const post = (message: ClientRequest | HostCommand): Promise<unknown> | null => {
    const stamped = { protocol_version: PROTOCOL_VERSION, game_id: play.gameId, ...message };
    if (link.request) {
      return link.request(stamped);
    }
    link.send(stamped);
    return null;
  };
  /** Sends when the game takes a message, and says whether it did. */
  const send = (message: ClientRequest | HostCommand) => {
    if (!play.ready()) {
      return false;
    }
    post(message)?.catch(() => undefined);
    return true;
  };
  const settle = () => {
    if (resync !== null) {
      clearTimeout(resync);
      resync = null;
    }
  };
  const fail = (message: string) => {
    if (failed !== null) {
      return;
    }
    failed = message;
    settle();
    link.close();
    say({ kind: "error", message });
  };
  const eventOf = (): Update | null => {
    if (!latest) {
      return null;
    }
    const { version: _, ...event } = latest;
    const seat = play.stepping?.() ?? null;
    const told = { ...event, rolls: actionRolls(rolls), events };
    return seat === null ? told : { ...told, stepping: { seat } };
  };
  /**
   * Tells the game as it is now, once more: what was staged against it starts again. `plan`
   * and `undone` are told this once, and no listener that comes later gets them.
   */
  const again = (plan?: PlanStopped, undone?: PlanStep[]) => {
    const event = eventOf();
    if (event) {
      say({ ...event, ...(plan ? { plan } : {}), ...(undone?.length ? { undone } : {}) });
    }
  };

  /** The staged work of the seat, kept by the game for every connection of the seat. */
  const staged = seatSync((slot, base, value) => {
    if (
      !send({
        type: "put_staged",
        slot,
        base_revision: base,
        ...(value === null ? {} : { value }),
      } as ClientRequest)
    ) {
      return false;
    }
    play.afterPut?.();
    return true;
  }, play.legacyStaged);

  const onState = (message: StateMsg, snapshot: boolean) => {
    if (snapshot) {
      const here = play.catalog?.content;
      if (play.catalog && here !== message.content) {
        return fail(
          `The server plays with other cards than this page (${short(message.content)} there, ${short(here)} here). Reload the page; if that does not help, the page and the server are of different builds.`,
        );
      }
      play.snapshot?.();
      // A snapshot has every event of the game: an undo took the later ones back.
      rolls = logOf(message.events);
      events = eventsOf(message.events);
      board = null;
    }
    settle();
    const told = modesOf(message.reaction_modes ?? {});
    if (JSON.stringify(told) !== JSON.stringify(modes)) {
      modes = told;
      say({ kind: "modes", modes: { ...modes } });
    }
    for (const note of passedOf(message)) {
      if (!noted.has(note.id)) {
        noted.add(note.id);
        // One note for the windows of one reason.
        for (let count = 0; count < (play.passedEach ? note.count : 1); count += 1) {
          say({ kind: "passed", cards: note.cards });
        }
      }
    }
    const update = updateOf(message);
    if (import.meta.env.DEV) {
      board = checkBoard(board, update.view, events);
    }
    // The game tells the game again when only a card setting changed. The page then keeps what
    // is staged: nothing of the game is new.
    const same =
      !snapshot &&
      !offline &&
      !stopped &&
      latest?.version === message.game_version &&
      latest.update.pending_choice?.nonce === update.pending_choice?.nonce;
    latest = {
      kind: "update",
      update,
      canUndo: message.history?.can_undo === true,
      version: message.game_version,
    };
    offline = false;
    undoing = false;
    if (!same) {
      const plan = stopped ?? undefined;
      stopped = null;
      held = null;
      again(plan, message.undone);
    }
  };

  const receive = (message: ServerMessage) => {
    if (message.protocol_version !== PROTOCOL_VERSION) {
      return fail(
        `The server speaks protocol ${message.protocol_version} and this page protocol ${PROTOCOL_VERSION}. Reload the page.`,
      );
    }
    switch (message.type) {
      case "initial_snapshot":
        return onState(message, true);
      case "state_update":
        return onState(message, false);
      case "turn_status":
        // Of the game as this side has it: a status of a later version comes with its update.
        if (latest?.version === message.game_version) {
          latest = { ...latest, update: { ...latest.update, turn_status: message.status } };
          say({ kind: "status", status: message.status });
        }
        return;
      case "action_rejected": {
        sent = null;
        undoing = false;
        if (message.reason.reason === "answered_elsewhere") {
          // Another tab of the seat, or the server for a ready draft, answered first: the update
          // that shows the answer is on its way, and nothing went wrong.
          return;
        }
        say({ kind: "rejected", reason: rejection(message.reason) });
        again();
        const { reason } = message.reason;
        if (
          play.resync &&
          (reason === "stale_version" || reason === "stale_nonce" || reason === "no_pending_choice")
        ) {
          // This side is behind the game. The update that says so is usually on its way; when
          // none comes, the game is asked for again.
          const ask = play.resync;
          resync ??= setTimeout(() => {
            resync = null;
            ask();
          }, RESYNC_MS);
        }
        return;
      }
      case "error":
        if (asked && replies) {
          // The engine could not play the draft: the reason is its outcome.
          refusal = message.message;
          return;
        }
        // Before the first update the game refused the subscription: there is no game to show.
        if (!latest) {
          return fail(play.refused?.(message) ?? message.message);
        }
        return say({
          kind: "rejected",
          reason:
            play.stagedRefused !== undefined && staged.waiting()
              ? play.stagedRefused
              : message.message,
        });
      case "event":
        rolls = withEvent(rolls, message.entry);
        events = withPublicEvent(events, message.entry);
        return;
      case "seat_staged":
        if (play.player) {
          staged.receive(message);
        }
        return;
      case "draft_outcome":
        if (asked?.id === message.request_id) {
          const { script, scope } = asked;
          asked = null;
          say({ kind: "draft", script, scope, outcome: message.outcome });
        }
        return;
      case "draft_applied": {
        if (sent?.nonce !== message.nonce) {
          return;
        }
        const script = sent;
        sent = null;
        if (!message.stopped) {
          return;
        }
        const plan = planStoppedOf(message.stopped, script);
        if (plan.interrupted) {
          // The game went on: the update of where it is now comes next.
          stopped = plan;
        } else if (replies) {
          // The game may go back to the choice of the plan in the same reply.
          stopped = held = plan;
        } else {
          // Nothing was sent, so the game has no update: the same choice is open.
          again(plan);
        }
        return;
      }
      default:
        return;
    }
  };

  const unlink = link.subscribe((event) => {
    switch (event.kind) {
      case "message":
        return receive(event.message);
      case "offline":
        if (!offline) {
          offline = true;
          say({ kind: "offline" });
        }
        return;
      case "progress": {
        // A replay is told this often, so that the page shows how far it is.
        const now = performance.now();
        if (now - shown > REPLAY_SLICE_MS) {
          shown = now;
          say({ kind: "replaying", done: event.done, total: event.total });
        }
        return;
      }
      case "fatal":
        return fail(play.fatal?.(event.message) ?? event.message);
      default:
        return;
    }
  });

  // A reload, or a tab that is put away, does not wait for the next change: what was staged
  // last is written now.
  const leave = () => staged.flushNow();
  const hidden = () => document.visibilityState === "hidden" && staged.flushNow();
  if (play.player && typeof window !== "undefined") {
    window.addEventListener("pagehide", leave);
    document.addEventListener("visibilitychange", hidden);
  }

  const isPlayer = () => latest?.update.viewer.role === "player";
  /** The end of the reply of a plan: a refused plan that no update told is told now. */
  const replied = () => {
    if (held && stopped === held) {
      stopped = null;
      again(held);
    }
    held = null;
  };

  const transport: Transport = {
    catalog: play.catalog,
    odds: play.odds,
    groundOdds: play.groundOdds,
    fleetStanding: play.fleetStanding,
    capabilities: play.capabilities ?? {},
    ...(play.player ? { staged: staged.store } : {}),
    subscribe(listener) {
      listeners.add(listener);
      listener({ kind: "modes", modes: { ...modes } });
      if (failed !== null) {
        listener({ kind: "error", message: failed });
      } else {
        const event = eventOf();
        if (event) {
          listener(event);
        }
        if (offline) {
          listener({ kind: "offline" });
        }
      }
      return () => listeners.delete(listener);
    },
    submitChoice(nonce, optionId) {
      if (!latest || latest.update.pending_choice?.nonce !== nonce) {
        return;
      }
      send({ type: "submit_choice", nonce, expected_version: latest.version, option_id: optionId });
    },
    submitPlan(nonce, plan) {
      this.applyDraft(nonce, { choices: [], plan });
    },
    applyDraft(nonce, draft) {
      if (
        !play.ready() ||
        !latest ||
        latest.update.pending_choice?.nonce !== nonce ||
        !isPlayer()
      ) {
        return;
      }
      const script: DraftStep[] = [
        ...draft.choices.map((option_id): DraftStep => ({ kind: "choose", option_id })),
        ...(draft.plan?.steps ?? []),
      ];
      // The answer may come before the reply: what was sent is known before.
      sent = { nonce, destination: draft.plan?.destination ?? "", lead: draft.choices.length };
      post({ type: "apply_draft", nonce, expected_version: latest.version, script })?.then(
        replied,
        replied,
      );
    },
    draft(script, scope) {
      if ((latest && !isPlayer()) || (replies && (!latest || !play.ready()))) {
        // No turn to draft, or the game does not wait for this side. Later, not now, as every answer.
        queueMicrotask(() => say({ kind: "draft", script, scope, outcome: null }));
        return;
      }
      if (!play.ready()) {
        // While the game cannot be reached nothing is asked: the page asks again with the next update.
        return;
      }
      const id = `${++requests}`;
      asked = { id, script, scope };
      refusal = null;
      const unplayed = (error?: unknown) => {
        if (asked?.id !== id || !latest) {
          return;
        }
        // The engine did not play it: the reason is the outcome.
        asked = null;
        const reason =
          error instanceof Error
            ? error.message
            : (refusal ?? "The engine did not play the draft.");
        say({
          kind: "draft",
          script,
          scope,
          outcome: { update: latest.update, consumed: 0, stop: { kind: "failed", reason } },
        });
      };
      post({ type: "run_draft", request_id: id, script, ...(scope ? { scope } : {}) })?.then(
        () => unplayed(),
        unplayed,
      );
    },
    setReactionMode(key, mode) {
      if ((modes[key] ?? "always") === mode) {
        return;
      }
      if (play.modes) {
        modes = { ...modes };
        if (mode === "always") {
          delete modes[key];
        } else {
          modes[key] = mode;
        }
        play.modes.onModes?.({ ...modes });
        say({ kind: "modes", modes: { ...modes } });
      }
      // A window that is open stays as it was asked: the mode is of the windows after it. The
      // game tells the change with the game (`reaction_modes`).
      if (!send({ type: "set_reaction_mode", card: key, mode }) && play.modes) {
        unsent.add(key);
      }
    },
    undo() {
      // One undo at a time: the answer is the game that is played again.
      if (!latest?.canUndo || undoing || !play.ready()) {
        return;
      }
      undoing = true;
      post({ type: "undo", expected_version: latest.version })?.then(
        () => {
          undoing = false;
        },
        () => {
          undoing = false;
        },
      );
    },
    close() {
      settle();
      if (play.player && typeof window !== "undefined") {
        window.removeEventListener("pagehide", leave);
        document.removeEventListener("visibilitychange", hidden);
      }
      staged.close();
      unlink();
      link.close();
      listeners.clear();
      play.closed?.();
    },
  };

  return {
    transport,
    latest: () => latest,
    sending: () => sent !== null,
    undoing: () => undoing,
    again: () => again(),
    flushModes() {
      for (const key of unsent) {
        post({ type: "set_reaction_mode", card: key, mode: modes[key] ?? "always" })?.catch(
          () => undefined,
        );
      }
      unsent.clear();
    },
    tell: say,
    fail,
    lost() {
      settle();
      // What was on its way is lost. The snapshot after the next subscription says where the game is.
      sent = null;
      asked = null;
      undoing = false;
      staged.lost();
    },
  };
}
