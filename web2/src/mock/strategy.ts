// The strategy cards of the demo: only the history has one (`h-leadership`), read-only. The real
// game has every card (`src/session/select/`).
import type { BlockView, BoardTaskView } from "../model";

type State = any;

/** The printed text of each card, shown behind hover. */
export const STRATEGY_TEXT: Record<number, { primary: string; secondary: string }> = {
  // Leadership is still in the history of the demo (`h-leadership`).
  1: {
    primary: "Gain 3 command tokens. Then buy any number of tokens for 3 influence each.",
    secondary:
      "Buy command tokens. Each other player may buy tokens for 3 influence each. No strategy token is spent.",
  },
};

/** The public objectives, and whether the viewer fulfills each. The Objectives sheet shows the same. */
export const OBJECTIVES: {
  name: string;
  text: string;
  state: "scored" | "open" | "closed";
  reason: string;
  scored: string[];
}[] = [
  {
    name: "Corner the Market",
    text: "Control 4 planets that each have the same planet trait.",
    state: "closed",
    reason: "3 of 4 planets with the same trait",
    scored: ["hacan", "xxcha"],
  },
  {
    name: "Develop Weaponry",
    text: "Own 2 unit upgrade technologies.",
    state: "scored",
    reason: "Scored",
    scored: ["sol"],
  },
  {
    name: "Expand Borders",
    text: "Control 6 planets in non-home systems.",
    state: "open",
    reason: "8 of 6 planets",
    scored: ["hacan", "xxcha"],
  },
  {
    name: "Erect a Monument",
    text: "Spend 8 resources.",
    state: "closed",
    reason: "5 of 8 resources ready",
    scored: [],
  },
];
export interface Editor {
  blocks: BlockView[];
  /** Why the main button cannot send. Empty when it can. */
  problem: string;
  /** A secondary with nothing chosen: the viewer passes. */
  pass: boolean;
  /** A choice that Esc can clear. */
  staged: boolean;
  /** The label of the main button of the primary, when it is not "Resolve primary". */
  main?: string;
  cost: { amount: number; key: "res" | "inf" } | null;
  task: BoardTaskView | null;
  /** What the sent choice did, in game values. */
  result: string;
}

/** The editor of the half of the card that the viewer resolves: the demo has none left. */
export function strategyEditor(_state: State): Editor {
  return {
    blocks: [],
    problem: "",
    pass: false,
    staged: false,
    cost: null,
    task: null,
    result: "",
  };
}
