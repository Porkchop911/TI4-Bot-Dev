// A restack of command tokens (`details.mode: "restack"`, Predictive Intelligence): the game asks
// one move of one token at a time (options of kind `redistribute`, ids "<from>|<to>"), and a
// decline that ends it. Here it is the pool editor (`tokens.ts`): the pools are staged from the
// sheet of now, and the main button answers the moves that make the staged pools, one token from a
// pool that shrinks to a pool that grows, then the decline, by the ids of their options.
import type { BlockView } from "../../model";
import type { Choice } from "../wire";
import { type Editor, answered } from "./editor";
import { type PoolQuestion, type Pools, countIn, poolsOf, poolsView } from "./tokens";

/** The pools of the sheet, with the name of each in the ids of the moves. */
const POOLS: [keyof Pools, string][] = [
  ["tactic", "tactic"],
  ["fleet", "fleet"],
  ["strategic", "strategy"],
];

const idOf = (pools: Pools) => `${pools.tactic}|${pools.fleet}|${pools.strategic}`;

interface Question {
  pools: PoolQuestion;
  /** The moves that the game offers now, by id. */
  moves: Set<string>;
  /** The option that ends the restack. */
  done: string;
}

function questionOf(choice: Choice): Question | null {
  const details = choice.details ?? {};
  const now = poolsOf(details.pools);
  const total = details.total;
  const done = choice.options.find((option) => option.kind === "decline")?.id;
  const moving = choice.options.filter((option) => option.kind !== "decline");
  if (
    details.kind !== "command_tokens" ||
    details.mode !== "restack" ||
    !now ||
    typeof total !== "number" ||
    !done ||
    !moving.length ||
    moving.some((option) => option.kind !== "redistribute")
  ) {
    return null;
  }
  // Every arrangement of the tokens of the sheet: each of them is made by moves out of pools
  // that hold tokens now.
  const offered = new Set<string>();
  for (let tactic = 0; tactic <= total; tactic++) {
    for (let fleet = 0; fleet <= total - tactic; fleet++) {
      offered.add(idOf({ tactic, fleet, strategic: total - tactic - fleet }));
    }
  }
  return {
    pools: { now, total, offered, min: { tactic: 0, fleet: 0, strategic: 0 } },
    moves: new Set(moving.map((option) => option.id)),
    done,
  };
}

/** The moves of one token each that turn `now` into `after`. Null: the game does not offer one. */
function movesOf(question: Question, now: Pools, after: Pools): string[] | null {
  const from = POOLS.flatMap(([key, name]) =>
    Array.from({ length: Math.max(0, now[key] - after[key]) }, () => name),
  );
  const to = POOLS.flatMap(([key, name]) =>
    Array.from({ length: Math.max(0, after[key] - now[key]) }, () => name),
  );
  const moves = from.map((source, index) => `${source}|${to[index]}`);
  return from.length === to.length && moves.every((move) => question.moves.has(move))
    ? moves
    : null;
}

export interface RestackView {
  blocks: BlockView[];
  note: string;
  /** Why nothing can be sent yet. Null: the main button sends. */
  problem: string | null;
  label: string;
  /** Something is staged: Esc clears it. */
  changed: boolean;
  /** A decision of the pools: on a phone it is on the map. */
  onMap: boolean;
  /** The answers, by the ids of their options, in the order that the game asks. Null: not yet. */
  answers: string[] | null;
}

/** The restack of command tokens. Null for any other question. */
export function selectRestack(choice: Choice, draft: Pools | null): RestackView | null {
  const question = questionOf(choice);
  if (!question) {
    return null;
  }
  const { now } = question.pools;
  const pools = poolsView(question.pools, draft);
  const after = draft ?? now;
  const moves = pools.staged ? movesOf(question, now, after) : null;
  const problem =
    pools.left !== 0
      ? `${pools.note}.`
      : !moves
        ? "The game does not offer this arrangement."
        : null;
  return {
    blocks: [pools.block],
    note: problem ?? pools.note,
    problem,
    label: moves?.length ? "Redistribute" : "Skip",
    changed: !!draft,
    onMap: true,
    answers: moves && [...moves, question.done],
  };
}

/** One step of a counter of the pools. Null for another key or another question. */
export function countRestack(
  choice: Choice,
  draft: Pools | null,
  key: string,
  value: number,
): Pools | null {
  const question = questionOf(choice);
  return (question && countIn(question.pools, draft, key, value)?.pools) ?? null;
}

/** The restack of command tokens as an editor (`editors.ts`). */
export const restackEditor: Editor<Pools> = {
  view: (_update, choice, draft) => answered(selectRestack(choice, draft)),
  count: countRestack,
};
