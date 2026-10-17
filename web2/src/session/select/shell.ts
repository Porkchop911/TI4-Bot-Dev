// The whole screen from one update of the game and what the player staged in this page.
import type { ActionView, FooterView, ShellView } from "../../model";
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
  // A turn of the Log that the viewer opened: the board is as its open step left it, with its
  // system marked, and nothing on it is a control.
  const past = local.inspect
    ? (selectPastTurn(update, local.events ?? NO_EVENTS, local.inspect, catalog, printed) ??
      selectPastFlow(update, local.events ?? NO_EVENTS, local.inspect.turn, printed) ??
      selectPastPhase(update, local.events ?? NO_EVENTS, local.inspect.turn, printed))
    : null;
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
    },
    players: { ...selectPlayers(update, printed, cards.modes), pick: seatTask(update, local) },
    board: past
      ? {
          ...selectBoard(
            past.update,
            local.inspected,
            null,
            null,
            null,
            catalog,
            null,
            false,
            printed,
          ),
          ...(past.system
            ? {
                marked: past.system,
                // The board brings the system of the turn into view, with the systems around it.
                taskSystems: around(past.update, past.system),
                fitKey: `turn:${local.inspect?.turn}:${past.system}`,
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
    log: selectLog(update, local.events ?? NO_EVENTS, printed),
    inspect: past && local.inspect ? { turn: local.inspect.turn, action: past.action } : null,
    apply: local.draft?.apply ?? null,
    toast: null,
    asked: !!update.pending_choice || !!local.draft,
  };
}
