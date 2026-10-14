import { isPublicEvent } from "../protocol/eventVisibility.ts";
import type { BoardView, GameEvent, PendingChoiceDto, PlayerView } from "../protocol/types.ts";
import { systemTag } from "./systemFacts.ts";
import { scoringObjectiveMeta } from "./scoringOptions.ts";

/** What another player did, as a short corner notification. */
export type ActionToastCategory =
  | "card"
  | "activate"
  | "strategy"
  | "tech"
  | "score"
  | "vote"
  | "build"
  | "produce"
  | "move"
  | "trade"
  | "politics"
  | "explore"
  | "breakthrough"
  | "turn";

export interface ActionToast {
  /** Id of the (first) event this toast came from. */
  id: string;
  actor: string;
  category: ActionToastCategory;
  /** One phrase per merged action, without the actor ("played Sabotage"). */
  parts: string[];
  /** Number of merged actions; for produced units the number of units. */
  count: number;
}

export const MAX_ACTION_TOASTS = 3;
const MAX_LISTED_PARTS = 3;

interface Rule {
  re: RegExp;
  category: ActionToastCategory;
  /** The phrase shown for one action. */
  phrase: (m: RegExpExecArray, board?: BoardView | null) => string;
  count?: (m: RegExpExecArray) => number;
}

// The public log writes "<actor> <what happened>" (see `public_decision_facts` on the server).
// Only these facts become toasts; everything else (payments, hit assignment, "Done moving") is
// bookkeeping for the actor and stays in the event log.
const RULES: Rule[] = [
  { re: /^played (.+)$/, category: "card", phrase: (m) => `played ${m[1]}` },
  { re: /^activated #(\S+)$/, category: "activate", phrase: (m, board) => `activated system ${systemTag(m[1], board)}` },
  { re: /^picked (.+)$/, category: "strategy", phrase: (m) => `picked ${m[1]}` },
  { re: /^researched (.+)$/, category: "tech", phrase: (m) => `researched ${m[1]}` },
  {
    re: /^scored (?:objective )?(.+)$/,
    category: "score",
    // The log names the objective by id; the toast says it by its printed name.
    phrase: (m) => `scored ${objectiveName(m[1])}`,
  },
  { re: /^(voted .+|abstained from voting)$/, category: "vote", phrase: (m) => m[1] },
  { re: /^placed (.+) on (.+)$/, category: "build", phrase: (m) => `placed ${m[1]} on ${m[2]}` },
  {
    re: /^produced (\d+) (.+)$/,
    category: "produce",
    phrase: (m) => `produced ${m[1]} ${m[2]}`,
    count: (m) => Number(m[1]),
  },
  {
    re: /^moved (.+) from #(\S+) to #(\S+)$/,
    category: "move",
    phrase: (m, board) => `moved ${m[1]} from ${systemTag(m[2], board)} to ${systemTag(m[3], board)}`,
  },
  {
    re: /^(accepted|refused) the transaction$/,
    category: "trade",
    phrase: (m) => `${m[1]} the transaction`,
  },
  { re: /^chose (.+) as speaker$/, category: "politics", phrase: (m) => `chose ${m[1]} as speaker` },
];

function objectiveName(text: string): string {
  // "public objective Corner the Market with Imperial" is already worded; a bare id is not.
  if (/\s/.test(text)) return text;
  return scoringObjectiveMeta(text).name || text;
}

/** A card play or an objective score: shown the moment it happens, even inside a recapped turn. */
export function isCardOrScore(entry: GameEvent): boolean {
  if (entry.stage === "action selection") return false;
  const actor = entry.actor;
  const detail = entry.detail?.trim();
  if (!actor || !detail?.startsWith(`${actor} `)) return false;
  return /^(played|scored) /.test(detail.slice(actor.length + 1));
}

const TURN_PHRASES: Record<string, string> = {
  pass: "passed",
  tactical: "started a tactical action",
  strategic: "used a strategy card",
  component: "used a component action",
};

/**
 * Maps one public log entry to a toast. Never for the viewer's own actions (`viewerSeat`), never
 * for entries that are not public, and only for the facts listed in {@link RULES}. A spectator has
 * no `viewerSeat` and sees every player's actions.
 */
export function actionToastFromEvent(
  entry: GameEvent,
  viewerSeat?: string | null,
  board?: BoardView | null,
): ActionToast | null {
  if (!isPublicEvent(entry) || entry.event.kind !== "decision_resolved") return null;
  const actor = entry.actor;
  if (!actor || actor === viewerSeat) return null;

  if (entry.stage === "action selection" && entry.action_type && !entry.detail) {
    const phrase = TURN_PHRASES[entry.action_type];
    return phrase
      ? { id: entry.id, actor, category: "turn", parts: [phrase], count: 1 }
      : null;
  }

  const detail = entry.detail?.trim();
  if (!detail) return null;
  const text = detail.startsWith(`${actor} `) ? detail.slice(actor.length + 1) : null;
  if (text === null) return null;
  for (const rule of RULES) {
    const m = rule.re.exec(text);
    if (m)
      return {
        id: entry.id,
        actor,
        category: rule.category,
        parts: [rule.phrase(m, board)],
        count: rule.count ? rule.count(m) : 1,
      };
  }
  return null;
}

/**
 * Adds a toast to the visible ones. A burst by the same player in the same category (three
 * movements, two card plays) becomes one toast; at most `max` stay, the oldest is dropped.
 * Returns the new list and whether an existing toast was updated (its timer should restart).
 */
export function mergeActionToast(
  list: readonly ActionToast[],
  incoming: ActionToast,
  max = MAX_ACTION_TOASTS,
): ActionToast[] {
  const at = list.findIndex((t) => t.actor === incoming.actor && t.category === incoming.category);
  const next = [...list];
  if (at >= 0) {
    const prior = next.splice(at, 1)[0];
    next.push({
      ...prior,
      parts: [...prior.parts, ...incoming.parts],
      count: prior.count + incoming.count,
    });
  } else next.push(incoming);
  return next.slice(-max);
}

/** The text after the actor's name. */
export function actionToastText(toast: ActionToast): string {
  if (toast.parts.length === 1) return toast.parts[0];
  if (toast.category === "move") return `moved ${toast.parts.length} units`;
  if (toast.category === "produce") return `produced ${toast.count} units`;
  // Repeats ("played Sabotage" twice) are counted instead of listed twice.
  if (toast.category === "explore") return toast.parts.join("; ");
  const counted = new Map<string, number>();
  for (const part of toast.parts) counted.set(part, (counted.get(part) ?? 0) + 1);
  const phrases = [...counted].map(([part, n]) => (n > 1 ? `${part} ×${n}` : part));
  const shown = phrases.slice(0, MAX_LISTED_PARTS).join("; ");
  return phrases.length > MAX_LISTED_PARTS
    ? `${shown}; and ${phrases.length - MAX_LISTED_PARTS} more`
    : shown;
}

/** Other players whose victory points went up between two views. */
export function victoryPointToasts(
  before: readonly Pick<PlayerView, "id" | "victory_points">[],
  after: readonly Pick<PlayerView, "id" | "victory_points">[],
  viewerSeat?: string | null,
): ActionToast[] {
  const prior = new Map(before.map((p) => [p.id, p.victory_points]));
  const toasts: ActionToast[] = [];
  for (const player of after) {
    const was = prior.get(player.id);
    if (was === undefined || player.id === viewerSeat || player.victory_points <= was) continue;
    const gained = player.victory_points - was;
    toasts.push({
      id: `vp:${player.id}:${player.victory_points}`,
      actor: player.id,
      category: "score",
      parts: [`gained ${gained} VP (now ${player.victory_points})`],
      count: 1,
    });
  }
  return toasts;
}

/** The toast for a decision the engine resolved because only one option was legal. */
export function autoResolvedToast(
  choice: PendingChoiceDto | null | undefined,
  viewerSeat?: string | null,
): { decisionType: string; selectedValue: string } | null {
  if (!choice || !viewerSeat || choice.actor !== viewerSeat) return null;
  const options = choice.options.filter((o) => o.kind !== "decline" && o.id !== "decline");
  const only = options.length === 1 ? options[0] : null;
  if (!only?.auto_resolved) return null;
  return { decisionType: choice.prompt, selectedValue: only.label || only.id };
}
