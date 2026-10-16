// The action panel of a decision: the turn menu (`turn.ts`), a reaction window (`reaction.ts`), an
// editor of several questions (`editors.ts`), a research (`research.ts`), a choice on the map or
// in the player table, and the list of the game for every question that has none of these.
import type {
  ActionButtonView,
  ApplyView,
  BlockView,
  BoardTaskView,
  FlowActionView,
  MenuRowView,
  PlayerPickView,
  RowView,
} from "../../model";
import { plural } from "../../model";
import type { Review } from "../draft";
import type { LandingDraft } from "../landingDraft";
import type { MovementDraft } from "../movementDraft";
import type { KeptOdds } from "../movementOdds";
import type { GroundOddsKept } from "./invasion";
import type { MovementPlan } from "../movementPlan";
import type { KeptStanding, ProductionDraft } from "../productionDraft";
import type {
  Catalog,
  Choice,
  ChoiceOption,
  ExplorationCatalog,
  PoliticsHints,
  PublicRoll,
  ReactionModes,
  SessionUpdate,
} from "../wire";
import { exploringCard } from "./exploration";
import { systemLabel, tilesOf } from "./movement";
import { humanizeId, seatLabel, sentence } from "./names";
import { optionWords, promptWords } from "./notes";
import { printedActionCard, printedSource, strategyCardHelp } from "./printed";
import {
  interruptOf,
  interruptedAction,
  reactedAction,
  selectReaction,
  windowSystem,
} from "./reaction";
import { type EditorSlot, hintFieldsOf } from "./editor";
import { type OpenEditor, editorOf } from "./editors";
import { selectResearch } from "./research";
import type { Paying } from "./payment";
import { type Pools, selectTokens } from "./tokens";
import { type TurnMenu, isTurnMenu, selectTurn } from "./turn";

/** The private draft that is in view: what its toolbar and its screens say. */
export interface DraftInfo {
  canUndo: boolean;
  canRedo: boolean;
  canApply: boolean;
  applyHint: string;
  /** What "Apply to Live" will send. */
  apply: ApplyView | null;
  /** Why the game did not play the whole draft. */
  problem: string | null;
  /** The game is asked what it makes of the draft as it is now: the screen is of the draft before. */
  waiting: boolean;
  /** The step of the action that is in view. Null: the activation, or a question of an ability. */
  stage: "movement" | "invasion" | "production" | null;
  /** The steps that the game came to with this draft: each of them can be opened. */
  reached: ("movement" | "invasion" | "production")[];
  /** The step in view is in the draft. While it is not, it is decided in Live. */
  recorded: boolean;
  /** The step that the game comes to after the step in view. */
  next: "movement" | "invasion" | "production" | null;
  /** What the draft comes to after the step in view, when no step follows. */
  ends: string | null;
  /**
   * The check of the draft ends in the step in view, at a roll or at a step that a draft does not
   * play: what comes after it is played in Live.
   */
  boundary?: boolean;
  /** The live game is at the turn menu of the seat: the draft goes back to it. */
  fromMenu?: boolean;
  /** The recorded activation is open to choose another system: the system that it has. */
  editing?: string | null;
  /**
   * The entry that the game refused, and the step whose row marks it: its own step, or the last
   * step that the game came to when the action ends before it. Null: the activation.
   */
  review?: {
    entry: Review;
    at: "movement" | "invasion" | "production" | null;
    /** The recorded system of the activation. */
    system: string | null;
  } | null;
}

/**
 * The private draft of the viewer's secondary, while the strategic action of the update can be
 * drafted by the viewer. With no answer yet it is the draft that nothing was chosen in.
 */
export interface FollowInfo {
  /** What the player answered so far, as the game offered it. */
  answers: string[];
  /** The player marked the draft ready: it is sent when the game asks the seat. */
  ready: boolean;
  /** The editor is open at a question of the draft: the choice of the update is that question. */
  editing: boolean;
  /** The game is asked what it makes of the draft as it is now. */
  waiting: boolean;
  /** The draft has nothing more to decide: it can be marked ready. */
  over: boolean;
  /** Why the game did not play the whole draft. */
  problem: string | null;
  /** Where the draft ends before the secondary does. */
  end: string | null;
  /** The game asks the seat now, and the draft was not marked ready: it can still be sent as it is. */
  canSend: boolean;
}

/** What the player does in this page, between two updates. Nothing of it is sent. */
export interface LocalState {
  /** The system in the inspector. */
  inspected: string | null;
  /** The staged option of the pending choice, by id. */
  staged: string | null;
  /** The staged pools of a redistribution of command tokens. Null: the pools of now. */
  pools?: Pools | null;
  /** The staged payment of a paid research. Null: nothing staged. */
  researchPay?: Paying | null;
  /** What is staged in the editor of the open question (`editors.ts`). Null: nothing staged. */
  editor?: EditorSlot | null;
  /** The nonce of the choice that was answered. Its update is still the latest one. */
  sent: string | null;
  /** Why the game stopped, if it did. */
  error: string | null;
  /** The game cannot be reached: the update is the last one, and nothing can be sent. */
  offline?: boolean;
  /** The game is played again up to where it was; the update is out of date until it is there. */
  replaying: { done: number; total: number } | null;
  /** The last answer can be taken back. */
  canUndo: boolean;
  /** The staged movement of the open movement step. */
  movement: MovementDraft;
  /** The systems of the open movement that the player marked as done. */
  handled: string[];
  /** The staged landings of the open invasion step. */
  landing: LandingDraft;
  /** The force of the invasion facts that a tap lands: a choice of the hand. */
  landingKind: number | null;
  /** The staged production of the open production step: what is built, and what pays. */
  production: ProductionDraft;
  /** The build is chosen first; then the map is the payment. */
  productionStage: "build" | "pay";
  /** The step of the tactical action that is in view, when it is not the open one. */
  step: number | null;
  /**
   * The steps of a movement, of the landings or of a production that another decision stopped.
   * They are staged again when that step goes on.
   */
  remaining:
    | (MovementPlan & {
        /** From a draft that was applied, not from a plan that stopped. */ drafted?: boolean;
      })
    | null;
  /** What the last movement that was sent came to, when it did not reach its end. */
  planNote: { text: string; error: boolean } | null;
  /** The seat that the game waits for before it lets it decide: "Step" goes on. */
  stepping: string | null;
  /** Why the viewer cannot draft now. Null when the Draft tab opens. */
  draftLocked: string | null;
  /** The private draft, when it is in view: the update is then what the draft came to. */
  draft: DraftInfo | null;
  /** The private draft of the viewer's secondary. Null while there is none to draft. */
  follow?: FollowInfo | null;
  /**
   * The last answer of the engine for the odds of the space combat that the staged movement
   * starts. Of another movement while the answer for this one is on its way.
   */
  odds?: KeptOdds | null;
  /**
   * The last answers of the engine for the odds of the ground combats that the staged landings
   * start, by planet. Of other landings while the answer for these is on its way.
   */
  groundOdds?: GroundOddsKept;
  /**
   * The last answer of the engine for the fleet supply and the capacity after the staged build
   * of a production. Of another build while the answer for this one is on its way.
   */
  standing?: KeptStanding | null;
  /** The list of the turn menu that the player opened. Null: the picker. */
  menu?: TurnMenu | null;
  /** The private draft of a tactical action that the seat keeps, while it is not in view. */
  drafted?: { system: string | null } | null;
  /**
   * The public rolls of the action that is played now, in the order of the game. A draft has
   * none: it holds no dice.
   */
  rolls?: PublicRoll[];
}

/** The choice that the viewer can answer now. */
export const openChoice = (update: SessionUpdate, local: LocalState): Choice | null =>
  update.pending_choice &&
  update.pending_choice.nonce !== local.sent &&
  !local.error &&
  !local.offline &&
  !local.replaying
    ? update.pending_choice.choice
    : null;

/**
 * The editor of the open question of the live game (`editors.ts`). A draft and the draft of a
 * secondary have none: they record one answer at a time.
 */
export const openEditor = (
  update: SessionUpdate,
  local: LocalState,
  printed: Catalog | null = null,
): OpenEditor | null => {
  const choice = openChoice(update, local);
  return choice && !local.draft && !local.follow?.editing
    ? editorOf(update, choice, local.editor, printed)
    : null;
};

/** An option that declines what the question asks: it stays a row of the list. */
const isDecline = (option: ChoiceOption) => option.kind === "decline";

/**
 * The thing on the board that each option of a choice stands for, when every option but a
 * decline is a system (an activation, a system of a card, a system to recall from) or a planet
 * (the option names it). Null when the choice is not made on the board.
 */
export function boardPick(
  update: SessionUpdate,
  choice: Choice | null,
): { target: "system" | "planet"; of: Record<string, string> } | null {
  const options = (choice?.options ?? []).filter((option) => !isDecline(option));
  if (!options.length) {
    return null;
  }
  const tiles = update.view.board.map_tiles ?? [];
  const systems = new Set(tiles.map((tile) => tile.system_id));
  const planets = new Set(tiles.flatMap((tile) => (tile.planets ?? []).map((item) => item.id)));
  // An activation names its systems by their ids; a card or a strategy card does too, and
  // only a system of the map is chosen on the board.
  if (
    options.every(
      (option) =>
        option.kind === "activate" ||
        ((option.kind === "system" || option.kind === "recall") && systems.has(option.id)),
    )
  ) {
    return {
      target: "system",
      of: Object.fromEntries(options.map((option) => [option.id, option.id])),
    };
  }
  const planetOf = (option: ChoiceOption) => {
    const planet = option.payload?.planet;
    return typeof planet === "string" && planets.has(planet) ? planet : null;
  };
  if (options.every((option) => planetOf(option) !== null)) {
    return {
      target: "planet",
      of: Object.fromEntries(options.map((option) => [option.id, planetOf(option)!])),
    };
  }
  return null;
}

/** The option of a choice that a thing on the board stands for. */
export function optionOnBoard(
  update: SessionUpdate,
  choice: Choice | null,
  thing: string,
): string | null {
  const pick = boardPick(update, choice);
  return Object.entries(pick?.of ?? {}).find(([, id]) => id === thing)?.[0] ?? null;
}

/** The seat that each option of a choice stands for, as the engine names it (`details.seats`). */
export function seatsOf(choice: Choice | null): Record<string, string> | null {
  const seats = choice?.details?.seats;
  if (!seats || typeof seats !== "object") {
    return null;
  }
  const named = Object.entries(seats as Record<string, unknown>).filter(
    (entry): entry is [string, string] =>
      typeof entry[1] === "string" && !!choice?.options.some((option) => option.id === entry[0]),
  );
  return named.length ? Object.fromEntries(named) : null;
}

/** A choice of systems or planets is also made on the board. */
export function boardTask(update: SessionUpdate, local: LocalState): BoardTaskView | null {
  const choice = openChoice(update, local);
  // The map of an editor is its choice, or its payment.
  const editor = openEditor(update, local);
  if (editor) {
    return editor.view.task ?? null;
  }
  // The payment of a paid research is on the map, once its technology is chosen.
  const researching =
    choice && !local.draft && !local.follow?.editing
      ? selectResearch(update, choice, local.staged, null, local.researchPay ?? null)
      : null;
  if (researching?.task) {
    return researching.task;
  }
  const pick = boardPick(update, choice);
  if (!choice || !pick) {
    return null;
  }
  const staged = local.staged ? pick.of[local.staged] : undefined;
  if (pick.target === "planet") {
    const values = new Map(
      (update.view.board.map_tiles ?? []).flatMap((tile) =>
        (tile.planets ?? []).map(
          (planet) =>
            [
              planet.id,
              update.view.board.systems[tile.system_id]?.planets[planet.id]?.resources ??
                planet.resources,
            ] as const,
        ),
      ),
    );
    return {
      target: "planet",
      kind: "pick",
      interactive: true,
      values: Object.fromEntries(
        Object.values(pick.of).map((planet) => [planet, values.get(planet) ?? 0]),
      ),
      chosen: staged ? { [staged]: true } : {},
      verb: "Choose",
      unit: "resources",
    };
  }
  const facts = update.tactical?.kind === "activation" ? update.tactical : null;
  const activation = choice.options.every((option) => option.kind === "activate");
  return {
    target: "system",
    kind: "pick",
    interactive: true,
    values: Object.fromEntries(Object.values(pick.of).map((system) => [system, 0])),
    // Every system of the choice can be activated. The action does something in these: ships
    // can move there, the seat produces there, or its guns fire at the ships of another player.
    ...(facts && activation
      ? {
          highlight: facts.systems
            .filter((reach) => reach.ships > 0 || !!reach.production || !!reach.cannon)
            .map((reach) => reach.system),
        }
      : {}),
    chosen: staged ? { [staged]: true } : {},
    verb: activation ? "Activate" : "Choose",
    unit: "system",
  };
}

/** The player table is the choice: every option but a decline is a seat. */
export function seatTask(update: SessionUpdate, local: LocalState): PlayerPickView | null {
  const choice = openChoice(update, local);
  // An editor that chooses players has them in the player table (several seats at once).
  const editor = openEditor(update, local);
  if (editor) {
    return editor.view.pick ?? null;
  }
  const seats = seatsOf(choice);
  if (!choice || !seats) {
    return null;
  }
  const chosen = local.staged ? seats[local.staged] : undefined;
  return {
    verb: "Choose",
    seats: Object.fromEntries(Object.values(seats).map((seat) => [seat, { reason: null }])),
    chosen: chosen ? [chosen] : [],
  };
}

/** The option that a seat of the player table stands for. */
export const optionOfSeat = (choice: Choice | null, seat: string): string | null =>
  Object.entries(seatsOf(choice) ?? {}).find(([, item]) => item === seat)?.[0] ?? null;

const KEYS = "123456789";

/** The viewer's own component action: a card, an ability or a component that asks its target. */
function componentAction(update: SessionUpdate, choice: Choice): boolean {
  const source = choice.context?.source;
  const viewer = update.viewer.role === "player" ? update.viewer.seat : null;
  return (
    !!source &&
    ("ActionCard" in source || "FactionAbility" in source || "Content" in source) &&
    update.view.phase === "action" &&
    !!viewer &&
    update.view.active_player === viewer &&
    !update.secondary &&
    !update.view.board.active_system
  );
}

export function selectAction(
  update: SessionUpdate,
  local: LocalState,
  catalog: ExplorationCatalog | null = null,
  /** The printed names. Without it a faction is shown by its id. */
  printed: Catalog | null = null,
  /** The modes that the viewer set for its reactions. */
  modes: ReactionModes = {},
): FlowActionView {
  const choice = openChoice(update, local);
  const undo: ActionButtonView[] =
    local.canUndo && !local.replaying
      ? [{ label: "Undo", tone: "quiet", intent: { type: "undo" } }]
      : [];
  // The turn menu is the action picker. A draft and the draft of a secondary have their own.
  if (isTurnMenu(choice) && !local.draft && !local.follow?.editing) {
    return selectTurn(update, local, choice, printed, undo);
  }
  // The question in the names of the page: the deal of a transaction as the engine says it
  // (`details.transaction`), else its notes and factions (`promptWords`).
  const prompt = promptWords(choice, update, printed);
  // A reaction window has its trigger and its cards as printed. A draft has none: a window is
  // of the game as it runs.
  const window =
    choice && !local.draft && !local.follow?.editing
      ? selectReaction(update, choice, local.staged, printed, modes)
      : null;
  // The source is on top: an exploration card that asks, or the card or the ability of the
  // question, as printed. A strategy card is the heading of its own screen.
  const asker = choice?.context?.source;
  const exploring = choice ? exploringCard(update, catalog) : null;
  const source = !choice
    ? null
    : (exploring ??
      (window || !asker || "StrategyCard" in asker ? null : printedSource(asker, printed)));
  // The window is on top of the action that it reacts to, which stays in view under it.
  const reacted = window ? reactedAction(update, update.reaction?.trigger, printed) : null;
  const own = !!choice && !window && !local.draft && componentAction(update, choice);
  const onBoard = choice ? boardPick(update, choice) : null;
  const seats = seatsOf(choice);
  // A redistribution of command tokens is staged in the pools of the command sheet.
  const tokens = choice && !window ? selectTokens(choice, local.pools ?? null) : null;
  const stagedId = tokens ? tokens.staged : local.staged;
  const staged = choice?.options.find((option) => option.id === stagedId) ?? null;
  // A research is chosen in the technology tree.
  /** A draft records the answer; the live game sends it. */
  const recording = !!local.draft || !!local.follow?.editing;
  // A paid research is paid on the map in the live game; a draft records the research alone.
  const research =
    choice && !window
      ? selectResearch(
          update,
          choice,
          local.staged,
          printed,
          recording ? undefined : (local.researchPay ?? null),
        )
      : null;
  // A gain or a purchase of command tokens, a readying of planets, a placement of agenda cards
  // or of structures, a replenish, a recall of a command token, a score of an objective: one
  // screen of several questions of the live game, sent at once (`editors.ts`).
  const several = window ? null : (openEditor(update, local, printed)?.view ?? null);
  const tiles = tilesOf(update);
  const planetOf = (id: string) =>
    (update.view.board.map_tiles ?? [])
      .flatMap((tile) => (tile.planets ?? []).map((planet) => ({ planet, tile })))
      .find((item) => item.planet.id === id);
  /** The options that stay rows of the list: all of them, or those that the board or the table does not have. */
  const listed = (choice?.options ?? []).filter(
    (option) => !(onBoard?.of[option.id] ?? seats?.[option.id]),
  );
  const rows: MenuRowView[] = listed.map((option, index) => {
    // A deal, a note and a faction are named as the page names them, a card with its printed
    // text behind "?". An action card has its printed text in the row: the player chooses by it.
    const card = printedActionCard(option, printed);
    const named = optionWords(choice, option, update, printed);
    const help =
      option.kind === "strategy_card"
        ? // A strategy card of the strategy phase has its primary and its secondary behind "?".
          strategyCardHelp(option.id, printed)
        : named.help;
    return {
      key: KEYS[index] ?? "",
      title: card?.name ?? named.text,
      ...(card?.text ? { text: card.text } : {}),
      ...(help.length ? { help } : {}),
      selected: option.id === staged?.id,
      disabled: false,
      intent: { type: "chooseOption", option: option.id },
    };
  });
  const selected: RowView["control"] = {
    kind: "badge",
    pill: { tone: "draft", label: "Selected" },
  };
  // A new speaker is chosen in the player table.
  const speaker =
    !!seats &&
    !!choice &&
    choice.options.some((option) => option.kind === "speaker") &&
    choice.options.every((option) => option.kind === "speaker" || option.kind === "decline");
  // The choice of a thing on the board, or of a player: the panel has the choice, not the
  // candidates, which are on the board or in the player table.
  /** A planet as it is now: its values with the attachments, when they differ from the printed ones. */
  const now = (system: string, planet: string) =>
    update.view.board.systems[system]?.planets[planet];
  const chosenBlock = (): BlockView | null => {
    if (onBoard?.target === "planet") {
      const found = staged && onBoard.of[staged.id] ? planetOf(onBoard.of[staged.id]) : null;
      return {
        kind: "card",
        title: "Planet",
        aside: "Choose on the map",
        rows: found
          ? [
              {
                icon: "planet",
                title: [`${found.planet.label} · #${found.tile.system_id}`],
                subtitle: `${now(found.tile.system_id, found.planet.id)?.resources ?? found.planet.resources} / ${now(found.tile.system_id, found.planet.id)?.influence ?? found.planet.influence}`,
                link: [`pl:${found.planet.id}`, `sys:${found.tile.system_id}`],
                control: selected,
              },
            ]
          : [{ icon: "planet", title: ["No planet chosen"] }],
      };
    }
    if (
      onBoard?.target === "system" &&
      !choice?.options.every((item) => item.kind === "activate")
    ) {
      const system = staged ? onBoard.of[staged.id] : undefined;
      return {
        kind: "card",
        title: "System",
        aside: "Choose on the map",
        rows: system
          ? [
              {
                icon: "token",
                title: [systemLabel(tiles, system)],
                link: [`sys:${system}`],
                control: selected,
              },
            ]
          : [{ icon: "token", title: ["No system chosen"] }],
      };
    }
    if (seats && choice) {
      const seat = staged ? seats[staged.id] : undefined;
      return {
        kind: "card",
        title: speaker ? "Speaker" : "Player",
        aside: "Choose in the player table",
        rows: seat
          ? [
              {
                icon: "card",
                title: [{ seat }, ` ${seatLabel(update, seat, printed)}`],
                subtitle: speaker
                  ? `Speaker: ${seatLabel(update, update.view.speaker, printed)} → ${seatLabel(update, seat, printed)}`
                  : undefined,
                control: selected,
              },
            ]
          : [{ icon: "card", title: ["No player chosen"] }],
      };
    }
    return null;
  };
  // The limit that a unit is removed for, as the engine counts it.
  const details = choice?.details ?? {};
  const where =
    typeof details.system === "string" ? systemLabel(tiles, details.system) : "the system";
  const gauges: BlockView[] =
    typeof details.fleet_charged === "number" &&
    typeof details.fleet_limit === "number" &&
    details.reason !== "capacity"
      ? [
          {
            kind: "gauges",
            gauges: [
              {
                label: `Fleet supply in ${where}`,
                used: details.fleet_charged,
                total: details.fleet_limit,
              },
            ],
          },
        ]
      : typeof details.capacity_consumed === "number" &&
          typeof details.capacity_transport === "number"
        ? [
            {
              kind: "gauges",
              gauges: [
                {
                  label: `Capacity in ${where}`,
                  used: details.capacity_consumed,
                  total: details.capacity_transport,
                },
              ],
            },
          ]
        : [];
  const status = update.turn_status;
  const faction = (seat: string) => seatLabel(update, seat, printed);
  const stepping = !choice && !local.error && !local.replaying ? local.stepping : null;
  const viewer = update.viewer.role === "player" ? update.viewer.seat : null;
  const passed = !!update.view.players.find((player) => player.id === viewer)?.passed;
  // In the action phase another seat has the turn: who, and whom the game waits for.
  const turnOf =
    !choice &&
    !local.draft &&
    !local.error &&
    !local.offline &&
    !local.replaying &&
    update.view.phase === "action" &&
    viewer !== null &&
    update.view.active_player &&
    update.view.active_player !== viewer
      ? update.view.active_player
      : null;
  const idle = local.draft
    ? (local.draft.problem ??
      (local.draft.waiting
        ? "The game checks the draft."
        : "The draft has nothing to decide here."))
    : local.error
      ? local.error
      : local.offline
        ? "The game cannot be reached. The page tries again by itself."
        : local.replaying
          ? `Playing the saved game again: answer ${local.replaying.done} of ${local.replaying.total}.`
          : status.kind === "game_over"
            ? "The game is over."
            : stepping
              ? `${faction(stepping)} decides next.`
              : "The other seats are playing.";
  const what = source ?? null;
  const pickWord =
    onBoard?.target === "planet"
      ? "Choose a planet"
      : onBoard?.target === "system"
        ? "Choose a system"
        : seats
          ? "Choose a player"
          : null;
  const chosen = chosenBlock();
  // What a question says comes after it, with no choice (Politics' speaker question): action
  // cards that are drawn, and agenda cards that the game shows.
  const after = choice ? hintFieldsOf<PoliticsHints>(choice, "draws", "looks") : null;
  const draws = after?.draws ?? 0;
  const hand = update.view.players.find((player) => player.id === viewer)?.action_cards_count;
  const drawn: BlockView[] =
    draws > 0 && typeof hand === "number"
      ? [
          {
            kind: "card",
            title: "Action cards",
            rows: [
              {
                icon: "card",
                title: [`Draw ${plural(draws, "action card")}`],
                subtitle: `Action cards ${hand} → ${hand + draws}`,
              },
            ],
          },
        ]
      : [];
  const looks = after?.looks ?? 0;
  // An ability that asks the viewer in the middle of another action (Scheming in a Politics, an
  // agent in the strategic action of another seat, a card that was played in a window): a band
  // on top of that action, as a reaction window is. Its card is the head of the band; the action
  // under it is said in short. An agenda is the flow of its phase, an exploration card that is
  // drawn is the step that drew it, and the viewer's own component action is its own screen.
  const triggered =
    choice && source && !exploring && !own && !research && !several && !tokens && asker
      ? "Agenda" in asker
        ? null
        : source
      : null;
  const interrupted = triggered ? interruptedAction(update, printed) : null;
  /** The decision: the card that asks, the limit, the choice on the board, the list. */
  const decision = (head: boolean): BlockView[] => [
    // A component action has its card behind "?" of the heading; any other decision has its
    // source on top.
    ...(head && what && !own ? [{ kind: "source" as const, ...what }] : []),
    ...gauges,
    ...(chosen ? [chosen] : []),
    ...drawn,
    ...(rows.length || !chosen ? [{ kind: "menu" as const, rows }] : []),
  ];
  // The question of the ability without its name, which is the head of the band.
  const question = triggered
    ? sentence(
        prompt.text.toLowerCase().startsWith(`${triggered.name.toLowerCase()}: `)
          ? prompt.text.slice(triggered.name.length + 2)
          : prompt.text,
      )
    : null;
  const blocks: BlockView[] = interrupted
    ? interrupted.blocks
    : reacted
      ? reacted.blocks
      : !choice
        ? turnOf
          ? [
              {
                kind: "note",
                tone: "quiet",
                strong: passed ? "You passed." : `${faction(turnOf)} has the turn.`,
                text: passed
                  ? "No more actions for you in this round."
                  : stepping
                    ? `${faction(stepping)} decides next.`
                    : local.draftLocked
                      ? "Your next tactical action can be drafted later."
                      : "You can prepare your next tactical action in Draft.",
              },
            ]
          : [{ kind: "note", tone: local.error ? "accent" : "quiet", text: idle }]
        : research
          ? [research.block]
          : several
            ? several.blocks
            : tokens
              ? [tokens.block]
              : decision(true);
  return {
    kind: "flow",
    // The heading says the rest. A private draft is said by the bar of the game.
    context: local.draft
      ? "Tactical action"
      : (interrupted ?? reacted)
        ? (interrupted ?? reacted)!.context
        : own && what
          ? "Component action"
          : turnOf
            ? passed
              ? "You passed"
              : "Turn ended"
            : null,
    badge:
      !local.draft && (staged || several?.changed) ? { tone: "draft", label: "Not sent" } : null,
    tabs: [],
    selected: 0,
    current: null,
    contentKey: update.pending_choice?.nonce ?? "idle",
    interrupt: window
      ? interruptOf(window, windowSystem(update, choice))
      : triggered
        ? {
            label: `${triggered.type} · ${triggered.name}`,
            source: triggered,
            question: question !== triggered.name ? question : null,
            link: ((system) => (system ? [`sys:${system}`] : []))(windowSystem(update, choice)),
            blocks: decision(false),
          }
        : null,
    closing: [],
    heading: local.draft
      ? "Draft"
      : (interrupted ?? reacted)
        ? (interrupted ?? reacted)!.heading
        : own && what
          ? what.name
          : choice
            ? prompt.text
            : turnOf
              ? `${faction(turnOf)}’s turn`
              : "Waiting",
    pill: choice
      ? {
          tone: "live",
          label: window
            ? "Your decision · Reaction"
            : own && pickWord
              ? `Your decision · ${pickWord}`
              : "Your decision",
        }
      : null,
    trail:
      own && what
        ? [
            { label: `Play ${what.name}`, status: "complete" as const },
            {
              label: sentence(pickWord?.replace("Choose a ", "") ?? "decision"),
              status: "active" as const,
            },
          ]
        : [],
    help:
      own && what?.text
        ? [{ title: `${what.type} · ${what.name}`, text: what.text }]
        : triggered
          ? []
          : prompt.help,
    blocks,
    footer: {
      note: local.planNote
        ? local.planNote.text
        : !choice
          ? ""
          : several
            ? several.note
            : tokens
              ? tokens.note
              : research?.problem
                ? research.problem
                : staged && research?.change
                  ? `Selected: ${research.name(staged.id)} · ${research.change}`
                  : staged
                    ? `Selected: ${window?.rows.find((row) => row.selected)?.title ?? (research ? research.name(staged.id) : optionText(update, choice, staged, onBoard, seats, printed))}`
                    : window
                      ? ""
                      : research
                        ? `${research.open} ${research.open === 1 ? "technology can" : "technologies can"} be researched.`
                        : pickWord
                          ? `${pickWord} ${onBoard ? "on the map" : "in the player table"}.`
                          : `Choose one of ${choice.options.length}.`,
      error: !!local.planNote?.error,
      ...(several?.payment
        ? { payment: several.payment }
        : research?.payment
          ? { payment: research.payment }
          : {}),
      actions: [
        ...(stepping
          ? [
              {
                label: "Step",
                tone: "primary" as const,
                key: "Enter",
                intent: { type: "simulate" as const },
              },
            ]
          : []),
        ...undo,
        ...(choice
          ? [
              // The pools are cleared when the player changed them.
              ...((several ? several.changed : tokens ? local.pools : staged)
                ? [
                    {
                      label: "Clear",
                      tone: "quiet" as const,
                      key: "Escape",
                      intent: { type: "clearChoice" as const },
                    },
                  ]
                : []),
              window?.pass && !staged
                ? {
                    // The main button of a reaction window passes, until a row is staged.
                    label: "Pass",
                    tone: "primary" as const,
                    key: "Enter",
                    intent: { type: "reaction" as const, action: "pass" as const },
                  }
                : window?.use
                  ? {
                      label: window.use,
                      tone: "primary" as const,
                      key: "Enter",
                      intent: { type: "reaction" as const, action: "play" as const },
                    }
                  : several
                    ? {
                        label: several.label,
                        tone: "primary" as const,
                        disabled: !!several.problem,
                        key: "Enter",
                        intent: { type: "flow" as const, action: "resolve" as const },
                      }
                    : research?.pass && !staged
                      ? {
                          // Nothing chosen in the tree is a pass, and the main button says so.
                          label: recording ? "Record · research nothing" : "Research nothing",
                          tone: "primary" as const,
                          key: "Enter",
                          intent: { type: "flow" as const, action: "resolve" as const },
                        }
                      : {
                          // A draft sends nothing: the answer is recorded.
                          label: recording
                            ? "Record"
                            : looks > 0
                              ? "Send · look at the agenda deck"
                              : "Send",
                          tone: "primary" as const,
                          disabled: !staged || !!research?.problem,
                          key: "Enter",
                          intent: { type: "flow" as const, action: "resolve" as const },
                        },
            ]
          : []),
      ],
    },
  };
}

/** What a staged option is, in words: the thing on the board or the player that it stands for. */
function optionText(
  update: SessionUpdate,
  choice: Choice | null,
  option: ChoiceOption,
  onBoard: ReturnType<typeof boardPick>,
  seats: Record<string, string> | null,
  printed: Catalog | null,
): string {
  const thing = onBoard?.of[option.id];
  if (thing && onBoard?.target === "system") {
    const tiles = tilesOf(update);
    return systemLabel(tiles, thing);
  }
  if (thing) {
    const planet = (update.view.board.map_tiles ?? [])
      .flatMap((tile) => tile.planets ?? [])
      .find((item) => item.id === thing);
    return planet?.label ?? humanizeId(thing);
  }
  const seat = seats?.[option.id];
  return seat
    ? seatLabel(update, seat, printed)
    : optionWords(choice, option, update, printed).text;
}
