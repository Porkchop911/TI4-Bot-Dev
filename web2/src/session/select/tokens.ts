// A redistribution of command tokens: the three pools of the command sheet, each with its
// tokens and a counter. The game says which arrangements are legal (the options of the
// question, ids "<tactic>|<fleet>|<strategy>"); the pools are staged here, and the arrangement
// that they make is the staged option when the game offers it.
import type { BlockView, RowView } from "../../model";
import { plural } from "../../model";
import type { Choice, CommandPools } from "../wire";

/** The three pools of a command sheet (the engine's `CommandPools`). */
export type Pools = CommandPools;

/** The name of one of the three pools. */
export type PoolKey = "tactic" | "fleet" | "strategic";

/** The kind of an option that sets the three pools (`REDISTRIBUTE_KIND` of the engine). */
const REDISTRIBUTE = "redistribute";

const POOLS: [PoolKey, "t" | "f" | "s", string, string][] = [
  ["tactic", "t", "Tactic pool", "Tactic"],
  ["fleet", "f", "Fleet pool", "Fleet"],
  ["strategic", "s", "Strategy pool", "Strategy"],
];

export interface TokensView {
  block: BlockView & { kind: "card" };
  /** The option of the staged arrangement. Null while the game does not offer it. */
  staged: string | null;
  /** The footer: what is left to place, or the change in game values. */
  note: string;
  /** The tokens that are still to place; less than 0: too many are placed. */
  left: number;
}

const idOf = (pools: Pools) => `${pools.tactic}|${pools.fleet}|${pools.strategic}`;
const sumOf = (pools: Pools) => pools.tactic + pools.fleet + pools.strategic;

/** The arrangement of an option id, or null when the id is not one. */
const parse = (id: string): Pools | null => {
  const parts = id.split("|").map(Number);
  return parts.length === 3 && parts.every(Number.isInteger)
    ? { tactic: parts[0], fleet: parts[1], strategic: parts[2] }
    : null;
};

/** The pools that a question states, from its details, with the seat's fleet supply bonus. */
export const poolsOf = (value: unknown): Pools | null => {
  const pools = value as Partial<Record<keyof Pools, unknown>> | null;
  if (
    !pools ||
    typeof pools.tactic !== "number" ||
    typeof pools.fleet !== "number" ||
    typeof pools.strategic !== "number"
  ) {
    return null;
  }
  const bonus = pools.fleet_supply_bonus;
  return {
    tactic: pools.tactic,
    fleet: pools.fleet,
    strategic: pools.strategic,
    ...(typeof bonus === "number" && bonus > 0 ? { fleet_supply_bonus: bonus } : {}),
  };
};

/** The pools of a redistribution: the sheet of now, the tokens to arrange, and what is offered. */
export interface PoolQuestion {
  now: Pools;
  total: number;
  offered: Set<string>;
  /** The smallest count of each pool among the offered arrangements. */
  min: Pools;
}

/**
 * The pools of a redistribution that offers the arrangements `ids` of `total` tokens, from the
 * sheet `now`. Null when they are not that.
 */
export function poolQuestion(now: unknown, total: unknown, ids: unknown[]): PoolQuestion | null {
  const sheet = poolsOf(now);
  if (!sheet || typeof total !== "number" || !ids.length) {
    return null;
  }
  const arrangements = ids.map((id) => (typeof id === "string" ? parse(id) : null));
  if (arrangements.some((pools) => !pools)) {
    return null;
  }
  const all = arrangements as Pools[];
  const least = (key: PoolKey) => Math.min(...all.map((pools) => pools[key]));
  return {
    now: sheet,
    total,
    offered: new Set(all.map(idOf)),
    min: { tactic: least("tactic"), fleet: least("fleet"), strategic: least("strategic") },
  };
}

/** A question that redistributes command tokens. Null for any other question. */
function questionOf(choice: Choice): PoolQuestion | null {
  const details = choice.details ?? {};
  if (
    details.kind !== "command_tokens" ||
    details.mode !== "redistribute" ||
    choice.options.some((option) => option.kind !== REDISTRIBUTE)
  ) {
    return null;
  }
  return poolQuestion(
    details.pools,
    details.total,
    choice.options.map((option) => option.id),
  );
}

/** The staged arrangement of a question: what the player staged, else the pools of now. */
export const stagingOf = (choice: Choice, staging: Pools | null): Pools | null => {
  const question = questionOf(choice);
  return question ? (staging ?? question.now) : null;
};

/**
 * One step of a counter (`pools.t`, `pools.f`, `pools.s`) of `question`: the pools after it,
 * and the arrangement that they make when the game offers it. Null for another key.
 */
export function countIn(
  question: PoolQuestion,
  staging: Pools | null,
  key: string,
  value: number,
): { pools: Pools; staged: string | null } | null {
  const pool = POOLS.find(([, short]) => `pools.${short}` === key)?.[0];
  if (!pool) {
    return null;
  }
  const pools = { ...(staging ?? question.now), [pool]: Math.max(0, value) };
  return { pools, staged: question.offered.has(idOf(pools)) ? idOf(pools) : null };
}

/** One step of a counter of a redistribution of command tokens. Null for any other question. */
export function countPools(
  choice: Choice,
  staging: Pools | null,
  key: string,
  value: number,
): { pools: Pools; staged: string | null } | null {
  const question = questionOf(choice);
  return question ? countIn(question, staging, key, value) : null;
}

/** The pool editor of a redistribution of command tokens. Null for any other question. */
export function selectTokens(choice: Choice, staging: Pools | null): TokensView | null {
  const question = questionOf(choice);
  return question ? poolsView(question, staging) : null;
}

/**
 * The rows of the three pools: the tokens of now and after, and a counter from `min` up to the
 * tokens that are `left` to place. The fleet pool says the seat's fleet supply bonus of `now`.
 */
export const poolRows = (now: Pools, after: Pools, min: Pools, left: number): RowView[] =>
  POOLS.map(([key, short, name]) => ({
    title: [name],
    ...(key === "fleet" && now.fleet_supply_bonus
      ? {
          subtitle: `+${now.fleet_supply_bonus} supply`,
          hint: `Your fleet supply is the fleet pool and ${plural(now.fleet_supply_bonus, "more non-fighter ship")} in each system.`,
        }
      : {}),
    tokens: { now: now[key], after: after[key], up: key === "fleet" },
    control: {
      kind: "counter",
      counter: {
        id: `pools.${short}`,
        value: after[key],
        min: min[key],
        max: after[key] + Math.max(0, left),
        label: `${name.toLowerCase()} token`,
      },
    },
  }));

/** The change of the pools in game values: "Tactic 2 → 3 · Fleet 3 · Strategy 2". */
export const poolChange = (now: Pools, after: Pools) =>
  POOLS.map(([key, , , word]) =>
    now[key] === after[key] ? `${word} ${now[key]}` : `${word} ${now[key]} → ${after[key]}`,
  ).join(" · ");

/** What is wrong with pools that do not hold their tokens: "Place 2 more tokens". Null: they do. */
export const placeProblem = (left: number) =>
  left > 0
    ? `Place ${plural(left, "more token")}`
    : left < 0
      ? `Remove ${plural(-left, "token")}`
      : null;

/** The pool editor of the pools of `question`, staged as `staging` (null: the sheet of now). */
export function poolsView(question: PoolQuestion, staging: Pools | null): TokensView {
  const { now, total, offered, min } = question;
  const after = staging ?? now;
  const placed = sumOf(after);
  const left = total - placed;
  const rows = poolRows(now, after, min, left);
  const id = idOf(after);
  return {
    block: {
      kind: "card",
      title: "Command pools",
      aside: `${placed} / ${total} placed`,
      bad: placed !== total,
      rows,
    },
    staged: offered.has(id) ? id : null,
    note: placeProblem(left) ?? poolChange(now, after),
    left,
  };
}
