// The whole screen from one update of the game and what the player staged in this page.
import type {
  ActionView,
  FooterView,
  InspectView,
  LogView,
  PillView,
  ShellView,
} from "../../model";
import type { Catalog, SessionUpdate } from "../wire";
import {
  type LocalState,
  boardTask,
  openChoice,
  openEditor,
  seatTask,
  selectAction,
} from "./action";
import { selectBoard } from "./board";
import { NO_EVENTS } from "../events";
import { selectLog } from "./log";
import { liveEntry, selectLiveLine, selectNow } from "./liveLine";
import { selectPastFlow } from "./pastFlow";
import { selectPastPhase } from "./pastPhase";
import { selectPastTurn } from "./pastTurn";
import { selectTokens } from "./tokens";
import { type CardSettings, NO_CARD_SETTINGS, selectActionCards } from "./cards";
import { selectExploration } from "./exploration";
import { phaseName, seatLabel } from "./names";
import { isEmpty } from "../productionDraft";
import { selectPlayers, selectSeats } from "./players";
import { selectScore } from "./score";
import { selectAgendas } from "./agendas";
import { selectSecrets } from "./secrets";
import { selectStrategy, showsStrategy } from "./strategy";
import { selectTactical, tacticalFacts } from "./tactical";
import { windowSystem } from "./reaction";
import { activeEffects } from "./effects";

function status(update: SessionUpdate, local: LocalState, printed: Catalog | null): string {
  const turn = update.turn_status;
  const faction = (seat: string) => seatLabel(update, seat, printed);
  if (local.error) {
    return "The game stopped";
  }
  if (local.offline) {
    return "Offline · reconnecting";
  }
  if (local.replaying) {
    return `Replaying ${local.replaying.done} of ${local.replaying.total}`;
  }
  if (local.draft) {
    return "Private draft";
  }
  // The question of a private draft of a secondary is not one of the game.
  if (openChoice(update, local) && !local.follow?.editing) {
    return "Your decision";
  }
  switch (turn.kind) {
    case "game_over":
      return turn.winner ? `Game over · ${faction(turn.winner)} wins` : "Game over";
    case "waiting_for_decision":
      return local.sent ? "Sent · the game goes on" : `Waiting for ${faction(turn.seat)}`;
    case "waiting_for_reactions":
      return "Waiting for reactions";
    case "active_turn":
      return `Turn of ${faction(turn.player)}`;
    case "phase_transition":
      return "Between phases";
  }
}

/** The footer with a main button that waits. */
/** A system and the systems next to it on the map: what the board frames for a turn of the Log. */
function around(update: SessionUpdate, system: string): string[] {
  const tiles = update.view.board.map_tiles ?? [];
  const at = tiles.find((tile) => tile.system_id === system);
  if (!at) {
    return [system];
  }
  return tiles
    .filter((tile) => {
      const [dq, dr] = [tile.q - at.q, tile.r - at.r];
      return Math.max(Math.abs(dq), Math.abs(dr), Math.abs(dq + dr)) <= 1;
    })
    .map((tile) => tile.system_id);
}

const held = (footer: FooterView): FooterView => ({
  ...footer,
  actions: footer.actions.map((button) =>
    button.tone === "primary" ? { ...button, disabled: true } : button,
  ),
});

/**
 * While the game is asked what it makes of a draft, the screen is of the draft before: its main
 * button waits for the answer.
 */
const waitingFor = (action: ActionView): ActionView =>
  action.kind === "flow"
    ? { ...action, footer: held(action.footer) }
    : { ...action, task: { ...action.task, footer: held(action.task.footer) } };

/**
 * A draft that the player opened from the turn menu goes back to it: Esc, when the footer has
 * nothing else for it.
 */
function withBack(action: ActionView, local: LocalState): ActionView {
  if (!local.draft?.fromMenu) {
    return action;
  }
  const add = (footer: FooterView): FooterView => {
    const taken = footer.actions.some((button) => button.key === "Escape");
    return {
      ...footer,
      actions: [
        {
          label: "Back to actions",
          tone: "quiet",
          ...(taken ? {} : { key: "Escape" }),
          intent: { type: "backToPicker" },
        },
        ...footer.actions,
      ],
    };
  };
  return action.kind === "flow"
    ? { ...action, footer: add(action.footer) }
    : { ...action, task: { ...action.task, footer: add(action.task.footer) } };
}

export function selectShell(
  update: SessionUpdate,
  local: LocalState,
  /** The printed cards. Without it they are shown by id. */
  printed: Catalog | null = null,
  cards: CardSettings = NO_CARD_SETTINGS,
): ShellView {
  const catalog = printed?.exploration ?? null;
  // The activation, the movement, the landings and the production have their own screens;
  // a strategic action is the list of its seats; every other decision is a list.
  const facts = tacticalFacts(update, local);
  const movement =
    facts?.kind === "movement" && local.step === null
      ? { facts, draft: local.movement, handled: local.handled, odds: local.odds ?? null }
      : null;
  const landing =
    facts?.kind === "invasion"
      ? {
          facts,
          draft: local.landing,
          chosen: local.landingKind,
          odds: local.groundOdds ?? {},
        }
      : null;
  const production =
    facts?.kind === "production"
      ? {
          facts,
          draft: local.production,
          stage: local.productionStage,
          standing: local.standing?.response ?? null,
        }
      : null;
  // A space combat is the step of its tactical action, at every seat: a seat that does not fight
  // watches it. A draft is not the game as it runs, so it has none.
  const combat = !facts && !local.draft ? (update.view.board.combat ?? null) : null;
  const action = facts
    ? selectTactical(update, local, facts, catalog, printed)
    : combat
      ? selectTactical(
          update,
          local,
          {
            kind: "combat",
            combat,
            flow: selectAction(update, local, catalog, printed, cards.modes),
            asked: !!openChoice(update, local) && !local.follow?.editing,
          },
          catalog,
          printed,
        )
      : showsStrategy(update, local)
        ? selectStrategy(update, local, catalog, printed)
        : selectAction(update, local, catalog, printed, cards.modes);
  const editing = openEditor(update, local, printed)?.view;
  // A redistribution of command tokens is a decision of the pools too: a phone has it on the map.
  const asked = openChoice(update, local);
  const pools = asked ? selectTokens(asked, local.pools ?? null) : null;
  const onMap = editing?.onMap ? editing.blocks : pools ? [pools.block] : null;
  // The system that an open window is about is marked on the board.
  const marked = action.interrupt ? windowSystem(update, openChoice(update, local)) : null;
  const events = local.events ?? NO_EVENTS;
  const opened = (inspect: { turn: string; step: number | null }) =>
    selectPastTurn(update, events, inspect, catalog, printed) ??
    selectPastFlow(update, events, inspect.turn, printed) ??
    selectPastPhase(update, events, inspect.turn, printed);
  const logged = selectLog(update, events, printed);
  // The turn or the phase that is played now: its step, on its row of the Log and on the line over
  // the board while another seat acts.
  const playing = liveEntry(logged);
  const watched = playing && local.inspect?.watch && local.inspect.turn === playing.id;
  // Watch closes when the turn that it shows ends.
  const inspect = local.inspect && (!local.inspect.watch || watched) ? local.inspect : null;
  // A turn of the Log that the viewer opened: the board is as its open step left it, with its
  // system marked, and nothing on it is a control. In Watch the board stays the live one.
  const past = inspect ? opened(inspect) : null;
  const playingNow = playing
    ? watched && past
      ? past
      : playing.kind === "turn"
        ? opened({ turn: playing.id, step: null })
        : null
    : null;
  const lastStep =
    playingNow?.action.kind === "tactical" ? (playingNow.action.tabs.at(-1)?.name ?? null) : null;
  const now = playing ? selectNow(update, playing, lastStep, printed) : null;
  const log: LogView = now
    ? {
        ...logged,
        entries: logged.entries.map((entry) =>
          entry === playing
            ? { ...entry, now: [now.step, now.detail].filter(Boolean).join(" · ") }
            : entry,
        ),
      }
    : logged;
  const history = past && !watched ? past : null;
  return {
    seats: selectSeats(update, printed),
    accent:
      local.draft ||
      local.staged ||
      (movement && Object.keys(local.movement).length) ||
      (landing && Object.keys(local.landing).length) ||
      (production && !isEmpty(local.production))
        ? "draft"
        : "live",
    toolbar: {
      round: update.view.round,
      turn: update.view.turn,
      phase: phaseName(update.view.phase),
      status: status(update, local, printed),
      workspace: local.draft ? "draft" : "live",
      draftLocked: local.draftLocked,
      draft: local.draft && {
        canUndo: local.draft.canUndo,
        canRedo: local.draft.canRedo,
        canApply: local.draft.canApply,
        applyHint: local.draft.applyHint,
      },
      effects: activeEffects(update, printed),
    },
    players: { ...selectPlayers(update, printed, cards.modes), pick: seatTask(update, local) },
    board: history
      ? {
          ...selectBoard(
            history.update,
            local.inspected,
            null,
            null,
            null,
            catalog,
            null,
            false,
            printed,
          ),
          ...(history.system
            ? {
                marked: history.system,
                // The board brings the system of the turn into view, with the systems around it.
                taskSystems: around(history.update, history.system),
                fitKey: `turn:${inspect?.turn}:${history.system}`,
              }
            : {}),
        }
      : {
          ...selectBoard(
            update,
            local.inspected,
            boardTask(update, local),
            movement,
            landing,
            catalog,
            production,
            !local.draft &&
              !!openChoice(update, local)?.options.some((option) => option.id === "tactical"),
            printed,
          ),
          // A decision of the map, or of the pools: a phone has its blocks on the map too.
          ...(onMap ? { blocks: onMap } : {}),
          ...(marked ? { marked } : {}),
          // Watch marks the system of the turn that it shows on the live board, and brings it into
          // view when it opens, unless a choice of the viewer has the board.
          ...(watched && past?.system && !marked
            ? {
                marked: past.system,
                ...(boardTask(update, local)?.interactive
                  ? {}
                  : {
                      taskSystems: around(update, past.system),
                      fitKey: `watch:${inspect?.turn}:${past.system}`,
                    }),
              }
            : {}),
        },
    action: withBack(
      local.draft?.waiting || local.follow?.waiting ? waitingFor(action) : action,
      local,
    ),
    reference: {
      objectives: selectScore(update, printed),
      technology: [],
      cards: [],
      actionCards: selectActionCards(update, printed, cards),
      secrets: selectSecrets(update, printed),
      agendas: selectAgendas(update, printed),
      exploration: selectExploration(update, catalog, printed),
      log: [],
    },
    log,
    inspect:
      past && inspect
        ? inspectOf(log, inspect.turn, past.action, update, printed, !!watched)
        : null,
    // The line is not there while a turn of the Log is open: Watch is what it says.
    live: inspect
      ? null
      : selectLiveLine(
          update,
          playing,
          now ?? { step: null, detail: null },
          !!openChoice(update, local),
          printed,
        ),
    apply: local.draft?.apply ?? null,
    toast: null,
    asked: !!update.pending_choice || !!local.draft,
  };
}

/** The rows of the Log that open: a tactical, strategic or component turn, a phase that is no action phase. */
const opens = (log: LogView) =>
  log.entries.filter((entry) =>
    entry.kind === "turn"
      ? entry.action === "tactical" || entry.action === "strategic" || entry.action === "component"
      : entry.phase !== "action",
  );

/**
 * A row of the Log that is open, in the history mode: which point of the game the screen shows
 * (the toolbar with "‹" and "›" to the rows around it, the stamp on the board). The
 * pill "Resolved" is gone: the mode says it. "Played now" stays on the row that is played now.
 */
function inspectOf(
  log: LogView,
  id: string,
  action: ActionView,
  update: SessionUpdate,
  printed: Catalog | null,
  watch: boolean,
): InspectView {
  const rows = opens(log);
  const at = rows.findIndex((entry) => entry.id === id);
  const entry = at >= 0 ? rows[at] : null;
  const resolved = (pill: PillView | null) => (pill?.label === "Resolved" ? null : pill);
  const shown: ActionView =
    action.kind === "tactical"
      ? { ...action, task: { ...action.task, pill: resolved(action.task.pill) } }
      : // A phase has its round once: in the toolbar.
        {
          ...action,
          pill: resolved(action.pill),
          context: entry?.kind === "phase" ? null : action.context,
        };
  let title = "";
  let stamp = "";
  if (entry?.kind === "turn") {
    const seat = seatLabel(update, entry.seat, printed);
    title = `Round ${entry.round} · Turn ${entry.turn} · ${seat}`;
    const step = action.kind === "tactical" ? action.tabs[action.selected]?.name : null;
    stamp = `Round ${entry.round} · Turn ${entry.turn} · ${step ? `after ${step}` : "end of the turn"}`;
  } else if (entry) {
    const phase = entry.label.split(" · ")[0];
    title = `Round ${entry.round} · ${phase}`;
    stamp = `Round ${entry.round} · end of the ${phase}`;
  }
  return {
    turn: id,
    action: shown,
    title,
    stamp,
    previous: at > 0 ? rows[at - 1].id : null,
    next: at >= 0 && at < rows.length - 1 ? rows[at + 1].id : null,
    watch,
  };
}
