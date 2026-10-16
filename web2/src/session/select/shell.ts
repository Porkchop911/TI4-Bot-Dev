// The whole screen from one update of the game and what the player staged in this page.
import type { ActionView, FooterView, ShellView } from "../../model";
import type { Catalog, SessionUpdate } from "../wire";
import { type LocalState, boardTask, openChoice, selectAction } from "./action";
import { selectBoard } from "./board";
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
    facts?.kind === "invasion" ? { facts, draft: local.landing, chosen: local.landingKind } : null;
  const production =
    facts?.kind === "production"
      ? {
          facts,
          draft: local.production,
          stage: local.productionStage,
        }
      : null;
  const action = facts
    ? selectTactical(update, local, facts, catalog, printed)
    : showsStrategy(update, local)
      ? selectStrategy(update, local, catalog, printed)
      : selectAction(update, local, catalog, printed, cards.modes);
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
      past: null,
      draft: local.draft && {
        canUndo: local.draft.canUndo,
        canRedo: local.draft.canRedo,
        canApply: local.draft.canApply,
        applyHint: local.draft.applyHint,
      },
    },
    players: selectPlayers(update, printed, cards.modes),
    board: selectBoard(
      update,
      local.inspected,
      boardTask(update, local),
      movement,
      landing,
      catalog,
      production,
    ),
    action: local.draft?.waiting || local.follow?.waiting ? waitingFor(action) : action,
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
    apply: local.draft?.apply ?? null,
    reveal: null,
    toast: null,
    announcement: "",
    asked: !!update.pending_choice || !!local.draft,
  };
}
