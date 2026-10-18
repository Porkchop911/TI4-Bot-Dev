// The recall of a command token from the board (options of kind `recall`), with what the game
// asks after it when the question says so (`details.redistribution`): the gain of a command
// token and the redistribution of every token of the sheet (Warfare's primary). The game asks
// the three one at a time; here they are one screen: the system is chosen on the map, the
// arrangement of the pools in the pool editor, and the main button answers all three at once,
// by the ids of their options.
import type { BlockView, BoardTaskView } from "../../model";
import type { Choice, PoolOptions, RedistributionHints, SessionUpdate } from "../wire";
import { systemLabel, tilesOf } from "./movement";
import {
  type PoolQuestion,
  type PoolKey,
  type Pools,
  countIn,
  poolQuestion,
  poolsView,
} from "./tokens";
import { type Editor, answered, changeControl, hintOf } from "./editor";

/** What the player staged: the system, and the pools after the gain and the redistribution. */
export interface RecallDraft {
  system: string | null;
  pools: Pools | null;
}

export const NO_RECALL: RecallDraft = { system: null, pools: null };

interface Question {
  systems: string[];
  /** The redistribution after the recall. Null: the game asks nothing after it. */
  pools: PoolQuestion | null;
  /** The option of the gain into each pool. Null: the game asks nothing after the recall. */
  gain: PoolOptions | null;
  gained: number;
}

function questionOf(choice: Choice): Question | null {
  if (!choice.options.length || choice.options.some((option) => option.kind !== "recall")) {
    return null;
  }
  const told = hintOf<RedistributionHints>(choice, "redistribution");
  return {
    systems: choice.options.map((option) => option.id),
    pools: told ? poolQuestion(told.pools, told.total, told.arrangements) : null,
    gain: told?.gain ?? null,
    gained: told?.gained ?? 0,
  };
}

export interface RecallView {
  blocks: BlockView[];
  task: BoardTaskView;
  note: string;
  /** Why nothing can be sent yet. Null: the main button sends. */
  problem: string | null;
  label: string;
  /** Something is staged: Esc clears it. */
  changed: boolean;
  /** A decision of the pools: on a phone it is on the map, with the system that is chosen there. */
  onMap: boolean;
  /** The answers, by the ids of their options, in the order that the game asks. Null: not yet. */
  answers: string[] | null;
}

const KEYS: PoolKey[] = ["tactic", "fleet", "strategic"];

/** The recall of a command token, with the gain and the redistribution after it. Null for any other question. */
export function selectRecall(
  update: SessionUpdate,
  choice: Choice,
  draft: RecallDraft | null,
): RecallView | null {
  const question = questionOf(choice);
  if (!question) {
    return null;
  }
  const staging = draft ?? NO_RECALL;
  const system =
    staging.system && question.systems.includes(staging.system) ? staging.system : null;
  const tiles = tilesOf(update);
  const pools = question.pools ? poolsView(question.pools, staging.pools) : null;
  const blocks: BlockView[] = [
    {
      kind: "card",
      title: "Command token",
      aside: "Choose on the map",
      rows: system
        ? [
            {
              icon: "token",
              title: [systemLabel(tiles, system)],
              subtitle: question.gained
                ? `The token returns to your reinforcements · you gain ${question.gained} command token${question.gained === 1 ? "" : "s"}`
                : "The token returns to your reinforcements",
              link: [`sys:${system}`],
              control: changeControl("system"),
            },
          ]
        : [{ icon: "token", title: ["No system chosen"] }],
    },
    ...(pools ? [pools.block] : []),
  ];
  // The gain goes into a pool that the arrangement fills; the redistribution then sets them all.
  const after = staging.pools ?? question.pools?.now ?? null;
  const into = question.pools
    ? KEYS.find((key) => after && after[key] > question.pools!.now[key] && question.gain)
    : undefined;
  const gainOption = into ? question.gain?.[into] : undefined;
  const problem = !system
    ? "Choose a system on the map."
    : pools && (!pools.staged || !gainOption)
      ? pools.left !== 0
        ? `${pools.note}.`
        : "The game does not offer this arrangement."
      : null;
  const note = problem ?? [systemLabel(tiles, system!), ...(pools ? [pools.note] : [])].join(" · ");
  return {
    blocks,
    task: {
      target: "system",
      kind: "pick",
      interactive: true,
      values: Object.fromEntries(question.systems.map((id) => [id, 0])),
      chosen: system ? { [system]: true } : {},
      verb: "Recall",
      unit: "system",
    },
    note,
    problem,
    label: "Resolve primary",
    changed: !!system || !!staging.pools,
    onMap: true,
    answers: problem
      ? null
      : [system!, ...(pools && gainOption ? [gainOption, pools.staged!] : [])],
  };
}

/** A system of the map is chosen, or given back. Null when it is not one to recall. */
export function chooseRecallSystem(
  choice: Choice,
  draft: RecallDraft | null,
  system: string,
): RecallDraft | null {
  const question = questionOf(choice);
  if (!question?.systems.includes(system)) {
    return null;
  }
  const staging = draft ?? NO_RECALL;
  return { ...staging, system: staging.system === system ? null : system };
}

/** One step of a counter of the pools. Null for another key or another question. */
export function countRecallPools(
  choice: Choice,
  draft: RecallDraft | null,
  key: string,
  value: number,
): RecallDraft | null {
  const question = questionOf(choice);
  const counted = question?.pools && countIn(question.pools, draft?.pools ?? null, key, value);
  return counted ? { ...(draft ?? NO_RECALL), pools: counted.pools } : null;
}

/** "Change" of the system. */
export function clearRecallPart(
  choice: Choice,
  draft: RecallDraft | null,
  part: string,
): RecallDraft | null {
  return questionOf(choice) && draft && part === "system" ? { ...draft, system: null } : null;
}

/** Esc clears the last choice: the pools, then the system. */
export function clearLastRecall(choice: Choice, draft: RecallDraft | null): RecallDraft | null {
  if (!questionOf(choice) || !draft) {
    return null;
  }
  return draft.pools ? { ...draft, pools: null } : draft.system ? NO_RECALL : null;
}

/** The recall of a command token as an editor (`editors.ts`). */
export const recallEditor: Editor<RecallDraft> = {
  view: (update, choice, draft) => answered(selectRecall(update, choice, draft)),
  system: chooseRecallSystem,
  count: countRecallPools,
  part: clearRecallPart,
  last: clearLastRecall,
};
