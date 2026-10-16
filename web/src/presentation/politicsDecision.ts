import type { ChoiceOption, GameView, PendingChoiceDto, PlayerView } from "../protocol/types.ts";
import { findActionCardMeta, humanizeId } from "../protocol/contentCatalog.ts";

/** What the client already knows about the table; enough to describe a candidate seat. */
export type DecisionTable = Pick<GameView, "players" | "seating_order" | "speaker"> &
  Partial<Pick<GameView, "table">>;

export const SPEAKER_ROLE_TEXT =
  "The speaker picks first in the strategy phase, votes last on agendas and breaks voting ties.";

const subtypeOf = (choice: Pick<PendingChoiceDto, "context">) => choice.context?.subtype ?? "";

export interface AgendaView {
  name: string;
  kind: string | null;
  target: string | null;
  /** The printed text, one paragraph per entry (the outcomes of a vote are in it). */
  text: string[];
}

const printedAgenda = (choice: Pick<PendingChoiceDto, "details">) => {
  const agenda = choice.details?.agenda;
  const record = agenda && typeof agenda === "object" && !Array.isArray(agenda) ? (agenda as Record<string, unknown>) : null;
  return (key: string) =>
    typeof record?.[key] === "string" && (record[key] as string).trim() ? (record[key] as string).trim() : null;
};

/**
 * The agenda a decision is about, as printed (`details.agenda`): the one just revealed for Quash,
 * Political Favor, Veto and the other "when an agenda is revealed" windows, the one being resolved
 * for Confusing Legal Text and the like, the one voted on for a tie-break. `null` when the decision
 * carries none, and for the Politics placement, which has its own card.
 */
export function describeDecisionAgenda(
  choice: Pick<PendingChoiceDto, "context" | "details">,
): (AgendaView & { revealed: boolean }) | null {
  if (subtypeOf(choice) === "politics_place_agenda") return null;
  const text = printedAgenda(choice);
  const name = text("name") ?? (text("id") ? humanizeId(text("id") as string) : null);
  if (!name) return null;
  const trigger = choice.context?.trigger?.kind ?? "";
  return {
    name,
    kind: text("type"),
    target: text("target"),
    text: [text("text1"), text("text2")].filter((line): line is string => line !== null),
    revealed: trigger !== "agenda_resolved" && trigger !== "votes_cast" && !/vote|tiebreak|recombination/.test(subtypeOf(choice)),
  };
}

/** The agenda being placed on top of or below the deck; `null` for any other decision. */
export function describeAgendaPlacement(
  choice: Pick<PendingChoiceDto, "prompt" | "context" | "details">,
): AgendaView | null {
  if (subtypeOf(choice) !== "politics_place_agenda") return null;
  const agenda = choice.details?.agenda;
  const record = agenda && typeof agenda === "object" ? (agenda as Record<string, unknown>) : null;
  const text = (key: string) =>
    typeof record?.[key] === "string" && (record[key] as string).trim()
      ? (record[key] as string).trim()
      : null;
  const fromPrompt = /^place (.+) where$/i.exec(choice.prompt.trim())?.[1] ?? null;
  const name = text("name") ?? (fromPrompt ? humanizeId(fromPrompt) : null);
  if (!name) return null;
  return {
    name,
    kind: text("type"),
    target: text("target"),
    text: [text("text1"), text("text2")].filter((line): line is string => line !== null),
  };
}

export interface SeatStanding {
  seat: string;
  faction: string;
  victoryPoints: number;
  commodities: number;
  /** 1 = the current speaker, then clockwise. */
  speakerOrder: number | null;
  isSpeaker: boolean;
}

export function seatStanding(table: DecisionTable, seat: string | undefined): SeatStanding | null {
  const player: PlayerView | undefined = table.players.find((entry) => entry.id === seat);
  if (!player) return null;
  const order = table.seating_order;
  const at = order.indexOf(player.id);
  const from = order.indexOf(table.speaker);
  return {
    seat: player.id,
    faction: humanizeId(player.faction),
    victoryPoints: player.victory_points,
    commodities: player.commodities,
    speakerOrder: at >= 0 && from >= 0 ? ((at - from + order.length) % order.length) + 1 : null,
    isSpeaker: player.id === table.speaker,
  };
}

/** The seat an option stands for: from `details.seats` (named options) or the id itself (Hacan). */
export function optionSeat(
  choice: Pick<PendingChoiceDto, "context" | "details">,
  option: ChoiceOption,
): string | undefined {
  const seats = choice.details?.seats;
  if (seats && typeof seats === "object") {
    const seat = (seats as Record<string, unknown>)[option.id];
    return typeof seat === "string" ? seat : undefined;
  }
  return subtypeOf(choice) === "leader_hacanagent_branch" && option.id !== "self"
    ? option.id
    : undefined;
}

const PLAYER_PICK = /_pick_player$/;

/** An action card (or ability) asking the actor to pick a player: Spy, Insubordination, Signal Jamming ... */
export const isPlayerPick = (choice: Pick<PendingChoiceDto, "context">): boolean =>
  PLAYER_PICK.test(subtypeOf(choice));

/**
 * The player a pick option stands for: the option id is a seat id, or (Signal Jamming) the name
 * of the faction that seat plays.
 */
export function playerPickSeat(option: Pick<ChoiceOption, "id" | "label">, table: DecisionTable): PlayerView | undefined {
  const id = option.id.toLowerCase();
  return (
    table.players.find((p) => p.id === option.id) ??
    table.players.find((p) => p.faction.toLowerCase() === id || humanizeId(p.faction).toLowerCase() === id)
  );
}

/** The action card behind a player pick, as printed; `null` when the decision has no card. */
export function describePickCard(
  choice: Pick<PendingChoiceDto, "context">,
): { name: string; text: string } | null {
  if (!isPlayerPick(choice)) return null;
  const source = choice.context?.source as Record<string, unknown> | undefined;
  const id = source?.ActionCard;
  if (typeof id !== "string") return null;
  const meta = findActionCardMeta(id);
  return { name: meta?.name ?? humanizeId(id), text: meta?.description ?? "" };
}

const count = (n: number, one: string, many = `${one}s`) => `${n} ${n === 1 ? one : many}`;

/** One line about a player the actor may pick: standing first, then what the card does to them. */
function playerPickNote(
  choice: Pick<PendingChoiceDto, "context" | "actor">,
  option: ChoiceOption,
  table: DecisionTable | null,
): OptionNote | null {
  const target = table ? playerPickSeat(option, table) : undefined;
  if (!target) return null;
  const subtype = subtypeOf(choice);
  const standing = `${target.victory_points} VP, ${count(target.trade_goods, "trade good")}, ${count(target.commodities, "commodity", "commodities")}`;
  let effect = "";
  if (subtype.startsWith("spy_")) {
    effect = ` Holds ${count(target.action_cards_count, "action card")}; you take one at random.`;
  } else if (subtype.startsWith("insubordination_")) {
    effect = ` Has ${count(target.tactic_tokens, "tactic token")}; you take one.`;
  }
  return { seat: target.id, text: `${standing}.${effect}` };
}

export interface OptionNote {
  /** One short line under the option's label. */
  text: string;
  /** The seat this line is about, for faction and colour. */
  seat?: string;
}

/** Display-only: a sub-line for one option of the three decisions; `null` elsewhere. */
export function optionNote(
  choice: Pick<PendingChoiceDto, "context" | "details" | "actor">,
  option: ChoiceOption,
  table: DecisionTable | null,
): OptionNote | null {
  const subtype = subtypeOf(choice);
  if (isPlayerPick(choice)) return playerPickNote(choice, option, table);
  const decline = option.kind === "decline" || option.id === "decline" || option.id === "done";
  if (subtype === "trade_choose_replenish" && decline) {
    return { text: "Nobody else replenishes; the Trade primary ends here." };
  }
  if (subtype === "leader_hacanagent_branch" && option.id === "self") {
    const me = table ? seatStanding(table, choice.actor) : null;
    return {
      text: me
        ? `Gain 2 commodities for yourself (you hold ${me.commodities}), up to your faction's commodity value.`
        : "Gain 2 commodities for yourself, up to your faction's commodity value.",
    };
  }
  if (!["politics_choose_speaker", "trade_choose_replenish", "leader_hacanagent_branch"].includes(subtype)) {
    return null;
  }
  const seat = optionSeat(choice, option);
  const standing = table ? seatStanding(table, seat) : null;
  if (!standing) return null;
  const vp = `${standing.victoryPoints} VP`;
  const held = `${standing.commodities} ${standing.commodities === 1 ? "commodity" : "commodities"} held`;
  if (subtype === "politics_choose_speaker") {
    const order =
      standing.speakerOrder !== null
        ? `, ${standing.isSpeaker ? "current speaker" : `${ordinal(standing.speakerOrder)} in speaker order now`}`
        : "";
    return {
      seat,
      text: `${vp}${order}. Becoming speaker: picks first, votes last, breaks ties.`,
    };
  }
  const gives =
    standing.commodities === 0
      ? "Replenish refills them to their full commodity value."
      : "Replenish refills them to their full commodity value; they gain the difference.";
  return { seat, text: `${held}, ${vp}. ${gives}` };
}

/** One line of reasoning above the Hacan Agent choice (and the replenish choice). */
export function replenishReason(
  choice: Pick<PendingChoiceDto, "context" | "actor" | "options" | "details">,
  table: DecisionTable | null,
): string | null {
  if (subtypeOf(choice) !== "leader_hacanagent_branch" || !table) return null;
  const targets = choice.options
    .filter((option) => option.id !== "self")
    .map((option) => seatStanding(table, option.id))
    .filter((entry): entry is SeatStanding => entry !== null);
  const me = seatStanding(table, choice.actor);
  if (targets.length === 0) return null;
  const fewest = Math.min(...targets.map((entry) => entry.commodities));
  const empty = targets.filter((entry) => entry.commodities === fewest);
  const gain = me ? ` (you hold ${me.commodities})` : "";
  return `Gain 2 commodities for yourself${gain}, or fill another player's. The emptiest candidate holds ${fewest}, so a replenish gives them the most${empty.length > 1 ? " (tied)" : ""}.`;
}

function ordinal(n: number): string {
  return n === 1 ? "1st" : n === 2 ? "2nd" : n === 3 ? "3rd" : `${n}th`;
}
