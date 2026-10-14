// The game on the server: `crates/ti4-server`, behind a websocket at the address of the page.
import type { PlanStep } from "./movementPlan";
import { type LegacyStaged, seatSync } from "./seatStore";
import { type Connection, type Socket, backoff, connect, pageSocket } from "./socket";
import type { PlanStopped, Transport, TransportEvent } from "./transport";
import { spaceOddsOf } from "./wasmTransport";
import {
  type Catalog,
  type ClientRequest,
  type DraftScope,
  type DraftStep,
  PROTOCOL_VERSION,
  type ReactionMode,
  type ReactionModes,
  type RejectionReason,
  type ServerMessage,
  type SessionUpdate,
  type SpaceOddsRequest,
  type StateMsg,
} from "./wire";

/** Which game of the server, and as whom. */
export interface OnlineGame {
  gameId: string;
  /** What the server gave the player when the game was made or joined. Null: a spectator. */
  playerSession: string | null;
  /** What an older page kept of the staged work of the player in this browser: put once, when the server has none. */
  legacyStaged?: LegacyStaged;
}

export { type Socket, openSocket } from "./socket";

export interface OnlinePlay {
  /** Opens the socket of a game. Without it: `/ws/games/<id>` at the address of the page. */
  open?(gameId: string): Socket;
  /** How long to wait before try `attempt` (from 0) to reach the game again, in ms. */
  retryAfter?(attempt: number): number;
  /**
   * Whether the server has the game. Asked when a socket did not open: the server refuses the
   * socket of a game that it does not have, and a socket cannot say why it did not open.
   * Without it: asked at the address of the page.
   */
  known?(gameId: string): Promise<boolean>;
}

/** What the transport says of a game that the server does not have. Nothing is tried after it. */
export const GAME_UNKNOWN = "The server no longer knows this game.";

/** The server closes a socket with this code when the game was replaced (a restore of the host, an undo of a seat). */
const REPLACED = 4001;
/** After a stale answer the update that explains it is on its way. Without it, the game is asked again. */
const RESYNC_MS = 1_000;

/** The server is at the address of the page, as the page itself. */
const gameSocket = (gameId: string): Socket =>
  pageSocket(`/ws/games/${encodeURIComponent(gameId)}`);

/** Only the answer "not found" says that the game is gone: a server that cannot be reached may come back. */
const pageKnows = (gameId: string): Promise<boolean> =>
  fetch(`/api/games/${encodeURIComponent(gameId)}/map`).then(
    (response) => response.status !== 404,
    () => true,
  );

const short = (digest: string | undefined) => (digest ? digest.slice(0, 12) : "none");

function rejection(reason: RejectionReason): string {
  switch (reason.reason) {
    case "stale_version":
    case "stale_nonce":
      return "The game went on before your answer arrived. Nothing was sent.";
    case "no_pending_choice":
      return "The game does not wait for this answer. Nothing was sent.";
    case "unauthorized_seat":
      return "The server does not take this answer from you. Nothing was sent.";
    case "unknown_option":
      return `The game does not offer "${reason.option_id}". Nothing was sent.`;
    case "validation_failed":
      return reason.message;
    case "answered_elsewhere":
      return "Your answer was given in another tab. Nothing was sent.";
    case "undo_barrier":
      return {
        revealed: "Something hidden was shown since your last decision. It cannot be taken back.",
        random: "Dice were rolled since your last decision. It cannot be taken back.",
        other_seat: "Another player decided since your last decision. It cannot be taken back.",
      }[reason.barrier];
  }
}

/** The part of a message of the server that is the update. `state` is large and not read. */
const updateOf = (message: StateMsg): SessionUpdate => ({
  viewer: message.viewer,
  view: message.view,
  pending_choice: message.pending_choice,
  turn_status: message.turn_status,
  tactical: message.tactical,
  exploration: message.exploration,
  action_cards: message.action_cards,
  secret_objectives: message.secret_objectives,
  score: message.score,
  agendas: message.agendas,
  secondary: message.secondary,
  reaction: message.reaction,
  seats: message.seats,
});

/**
 * Plays one game of the server. The first update is the snapshot that the server sends after
 * the subscription. `catalog` is the one of the engine of this page (`Engine.catalog`): the
 * server sends ids, and the game is played only when both have the same corpus. The odds of a
 * combat are computed by `module`, the engine of this page, as in a local game.
 */
export function createWsTransport(
  engine: { catalog: Catalog | null; module?: WebAssembly.Module },
  game: OnlineGame,
  play: OnlinePlay = {},
): Transport {
  type Update = Extract<TransportEvent, { kind: "update" }>;
  const listeners = new Set<(event: TransportEvent) => void>();
  const open = play.open ?? gameSocket;
  const retryAfter = play.retryAfter ?? backoff;
  const known = play.known ?? pageKnows;
  /** The game as the server told it last, and its version. */
  let latest: (Update & { version: number }) | null = null;
  /** Why the game cannot be played. Nothing is tried after it. */
  let failed: string | null = null;
  let offline = false;
  let closed = false;
  /** The modes of the seat, as the server told them last. */
  let modes: ReactionModes = {};
  let link: Connection | null = null;
  let attempt = 0;
  let resync: ReturnType<typeof setTimeout> | null = null;
  let sequence = 0;
  /** The newest draft that was asked. An answer for another request is late. */
  let asked: { id: string; script: DraftStep[]; scope?: DraftScope } | null = null;
  let requests = 0;
  /** The script that is being sent: its result is counted in entries, a plan in steps. */
  let sent: { nonce: string; destination: string; lead: number } | null = null;
  /** Told with the next update: where the last plan stopped. */
  let stopped: PlanStopped | null = null;
  /**
   * An undo was sent. The server plays the game again without the decision and closes the
   * socket as replaced: that is no loss of the game, and the snapshot after it is the answer.
   */
  let undoing = false;
  /** The notes of the server that were told. A note has an id so that a repeat can be dropped. */
  const noted = new Set<string>();
  /** The staged work of the seat, kept by the server for every tab of the seat. A spectator has none. */
  const player = game.playerSession !== null;
  const staged = seatSync(
    (slot, base, value) =>
      send({
        type: "put_staged",
        game_id: game.gameId,
        slot,
        base_revision: base,
        ...(value === null ? {} : { value }),
      }),
    game.legacyStaged,
  );

  const say = (event: TransportEvent) => {
    for (const listener of listeners) {
      listener(event);
    }
  };
  const send = (message: ClientRequest) =>
    link?.send(JSON.stringify({ protocol_version: PROTOCOL_VERSION, ...message })) ?? false;
  const settle = () => {
    if (resync !== null) {
      clearTimeout(resync);
      resync = null;
    }
  };
  /** The game cannot be played from here on. */
  const fail = (message: string) => {
    failed = message;
    settle();
    link?.close();
    say({ kind: "error", message });
  };
  /**
   * Tells the game as it is now, once more: what was staged against it starts again. `plan`
   * and `undone` are told this once, and no listener that comes later gets them.
   */
  const again = (plan?: PlanStopped, undone?: PlanStep[]) => {
    if (latest) {
      const { version: _, ...event } = latest;
      say({ ...event, ...(plan ? { plan } : {}), ...(undone?.length ? { undone } : {}) });
    }
  };

  const onState = (message: StateMsg, snapshot: boolean) => {
    if (snapshot) {
      const here = engine.catalog?.content;
      if (engine.catalog && here !== message.content) {
        return fail(
          `The server plays with other cards than this page (${short(message.content)} there, ${short(here)} here). Reload the page; if that does not help, the page and the server are of different builds.`,
        );
      }
      attempt = 0;
    }
    if (resync !== null) {
      clearTimeout(resync);
      resync = null;
    }
    const told = modesOf(message.reaction_modes ?? {});
    if (JSON.stringify(told) !== JSON.stringify(modes)) {
      modes = told;
      say({ kind: "modes", modes: { ...modes } });
    }
    for (const note of message.auto_resolved ?? []) {
      if (note.passed?.length && !noted.has(note.id)) {
        noted.add(note.id);
        say({ kind: "passed", cards: note.passed });
      }
    }
    const update = updateOf(message);
    // The server tells the game again when only a card setting changed. The page then keeps
    // what is staged: nothing of the game is new.
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
      again(plan, message.undone);
    }
  };

  const receive = (text: string) => {
    let message: ServerMessage;
    try {
      message = JSON.parse(text);
    } catch {
      return fail("The server sent a message that this page cannot read.");
    }
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
          reason === "stale_version" ||
          reason === "stale_nonce" ||
          reason === "no_pending_choice"
        ) {
          // This side is behind the game. The update that says so is usually on its way; when
          // none comes, the server is asked for the game again.
          resync ??= setTimeout(() => {
            resync = null;
            link?.again();
          }, RESYNC_MS);
        }
        return;
      }
      case "error":
        // Before the first update the server refused the subscription: there is no game to show.
        if (!latest) {
          return fail(
            message.kind === "unauthorized"
              ? "The server does not know the player of this page in this game."
              : `The server refused the game: ${message.message}`,
          );
        }
        // A write of the staged work that the server did not take is followed by what it keeps.
        return say({
          kind: "rejected",
          reason: staged.waiting()
            ? "The server did not keep what you staged. It stays in this tab."
            : message.message,
        });
      case "seat_staged":
        if (player) {
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
        const { destination, lead } = sent;
        sent = null;
        if (!message.stopped) {
          return;
        }
        // The server counts in entries of the script; a plan in the steps after the answers by id.
        const { applied, remaining, interrupted, reason } = message.stopped;
        const plan: PlanStopped = {
          destination,
          applied: Math.max(0, applied - lead),
          remaining: remaining.slice(Math.max(0, lead - applied)),
          interrupted,
          reason,
        };
        if (interrupted) {
          // The game went on: the update of where it is now comes next.
          stopped = plan;
        } else {
          // Nothing was sent, so the server has no update: the same choice is open.
          again(plan);
        }
        return;
      }
      default:
        return;
    }
  };

  link = connect(() => open(game.gameId), {
    opened() {
      send({
        type: "subscribe",
        game_id: game.gameId,
        ...(game.playerSession === null
          ? {}
          : { player_session: game.playerSession, staged: true }),
      });
    },
    // The server counts a player as present while its connection pings.
    beat: () => send({ type: "ping", sequence: ++sequence }),
    message: receive,
    lost(event) {
      settle();
      if (!event.opened) {
        // The server may have refused the socket because it does not have the game.
        void known(game.gameId).then((has) => {
          if (!has && !closed && failed === null) {
            fail(GAME_UNKNOWN);
          }
        });
      }
      // What was on its way is lost. The snapshot after the next subscription says where the game is.
      sent = null;
      asked = null;
      staged.lost();
      // The server replaced the game for the undo of this side: the game is not lost, and the
      // snapshot of the next subscription is what the undo left. Any other end is a loss.
      const undone = undoing && event.code === REPLACED;
      undoing = false;
      if (!offline && !undone) {
        offline = true;
        say({ kind: "offline" });
      }
      return event.code === REPLACED ? 0 : retryAfter(attempt++);
    },
  });

  // A reload, or a tab that is put away, does not wait for the next change: what was staged
  // last is written now.
  const leave = () => staged.flushNow();
  const hidden = () => document.visibilityState === "hidden" && staged.flushNow();
  if (player && typeof window !== "undefined") {
    window.addEventListener("pagehide", leave);
    document.addEventListener("visibilitychange", hidden);
  }

  const isPlayer = () => latest?.update.viewer.role === "player";

  return {
    catalog: engine.catalog,
    odds: (request: SpaceOddsRequest) =>
      engine.module
        ? spaceOddsOf(engine.module, request)
        : Promise.reject(new Error("This page has no engine for the odds.")),
    // The tools of a local game are not on the server.
    capabilities: {},
    ...(player ? { staged: staged.store } : {}),
    subscribe(listener) {
      listeners.add(listener);
      listener({ kind: "modes", modes: { ...modes } });
      if (failed !== null) {
        listener({ kind: "error", message: failed });
      } else {
        if (latest) {
          const { version: _, ...event } = latest;
          listener(event);
        }
        if (offline) {
          listener({ kind: "offline" });
        }
      }
      return () => listeners.delete(listener);
    },
    submitChoice(nonce, optionId) {
      const pending = latest?.update.pending_choice;
      if (!latest || pending?.nonce !== nonce) {
        return;
      }
      send({
        type: "submit_choice",
        game_id: game.gameId,
        nonce,
        expected_version: latest.version,
        option_id: optionId,
      });
    },
    submitPlan(nonce, plan) {
      this.applyDraft(nonce, { choices: [], plan });
    },
    applyDraft(nonce, draft) {
      if (!latest || latest.update.pending_choice?.nonce !== nonce || !isPlayer()) {
        return;
      }
      const script: DraftStep[] = [
        ...draft.choices.map((option_id): DraftStep => ({ kind: "choose", option_id })),
        ...(draft.plan?.steps ?? []),
      ];
      const asks = send({
        type: "apply_draft",
        game_id: game.gameId,
        nonce,
        expected_version: latest.version,
        script,
      });
      if (asks) {
        sent = { nonce, destination: draft.plan?.destination ?? "", lead: draft.choices.length };
      }
    },
    draft(script, scope) {
      if (latest && !isPlayer()) {
        // A spectator has no turn to draft. Later, not now, as every answer.
        queueMicrotask(() => say({ kind: "draft", script, scope, outcome: null }));
        return;
      }
      // While the game cannot be reached nothing is asked: the page asks again with the next update.
      const id = `${++requests}`;
      if (send({ type: "run_draft", game_id: game.gameId, request_id: id, script, scope })) {
        asked = { id, script, scope };
      }
    },
    setReactionMode(key, mode) {
      if ((modes[key] ?? "always") !== mode) {
        // The server tells the change with the game (`reaction_modes`): a `modes` event follows.
        send({ type: "set_reaction_mode", game_id: game.gameId, card: key, mode });
      }
    },
    undo() {
      // One undo at a time: the answer is the snapshot of the game that the server plays again.
      if (!latest?.canUndo || undoing) {
        return;
      }
      undoing = send({ type: "undo", game_id: game.gameId, expected_version: latest.version });
    },
    close() {
      closed = true;
      settle();
      if (player && typeof window !== "undefined") {
        window.removeEventListener("pagehide", leave);
        document.removeEventListener("visibilitychange", hidden);
      }
      staged.close();
      link?.close();
      listeners.clear();
    },
  };
}

/** The modes that are set, in the order of their keys: "always" is the mode of a key that is not there. */
function modesOf(told: Record<string, ReactionMode>): ReactionModes {
  const modes: ReactionModes = {};
  for (const key of Object.keys(told).sort()) {
    const mode = told[key];
    if (mode === "never" || mode === "ownturn") {
      modes[key] = mode;
    }
  }
  return modes;
}
