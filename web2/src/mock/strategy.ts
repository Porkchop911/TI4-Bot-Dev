// The eight strategy cards. Every card uses one frame: the parts of the card are blocks of one
// screen, the board and the player table serve the first part that is not complete, and nothing
// is sent before the main button. A secondary is the same editor as its part of the primary.
import type {
  BlockView,
  BoardTaskView,
  Intent,
  MenuRowView,
  TechCellView,
  TechColor,
} from "../model";
import { plural } from "../model";
import { PAY, SEATS, TECH } from "./data";
import { E } from "./loose";
import { openPayment } from "./select/shared";
import { TECHS, TECH_BY_ID, TECH_COLORS, type Tech } from "./tech";

type State = any;

/** The printed text of each card, shown behind hover. */
export const STRATEGY_TEXT: Record<number, { primary: string; secondary: string }> = {
  // Leadership is still in the history of the demo (`h-leadership`).
  1: {
    primary: "Gain 3 command tokens. Then buy any number of tokens for 3 influence each.",
    secondary:
      "Buy command tokens. Each other player may buy tokens for 3 influence each. No strategy token is spent.",
  },
  7: {
    primary: "Research 1 technology. Spend 6 resources to research 1 technology.",
    secondary: "Spend 1 token from your strategy pool and 4 resources to research 1 technology.",
  },
  8: {
    primary:
      "Immediately score 1 public objective if you fulfill its requirements. Gain 1 victory point if you control Mecatol Rex; otherwise, draw 1 secret objective.",
    secondary: "Spend 1 token from your strategy pool to draw 1 secret objective.",
  },
};

// ---- Demo data of the cards ----------------------------------------------------------------------

const ME = SEATS.sol;
const PLANETS = PAY.filter((source) => source.system);
const planetOf = (id: string) => PLANETS.find((source) => source.id === id)!;
const planetName = (id: string) => planetOf(id).label.replace("Exhaust ", "");
const names = (list: string[]) =>
  list.length < 2 ? (list[0] ?? "") : `${list.slice(0, -1).join(", ")} and ${list.at(-1)}`;

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
// ---- Shared parts --------------------------------------------------------------------------------

const paid = (pay: Record<string, boolean | number>, key: "res" | "inf"): number =>
  E.payTotal(pay, key);

const payTask = (
  cost: number,
  key: "res" | "inf",
  pay: Record<string, boolean>,
): BoardTaskView => ({
  payment: openPayment(cost, key, pay),
  target: "planet",
  kind: "pay",
  interactive: true,
  values: Object.fromEntries(PLANETS.map((source) => [source.id, source[key]!])),
  chosen: pay,
  verb: "Exhaust",
  unit: key === "inf" ? "influence" : "resource",
});

/** What a secondary costs: a token of the strategy pool. */
const tokenCost = () => `Strategy pool ${ME.tokens[2]} → ${ME.tokens[2] - 1}`;

/** A secondary with no choice: follow or pass, as a list. */
function followMenu(flow: any, title: string, effect: string): BlockView {
  const mine = flow.mine;
  const blocked = !ME.tokens[2];
  const row = (key: string, option: string, name: string, state?: string): MenuRowView => ({
    key,
    title: name,
    state,
    selected: mine.follow === (option === "follow"),
    disabled: option === "follow" && blocked,
    intent: { type: "chooseOption", option, group: "follow" },
  });
  return {
    kind: "menu",
    rows: [
      row("1", "follow", title, blocked ? "No strategy token" : `${effect} · ${tokenCost()}`),
      row("2", "pass", "Pass"),
    ],
  };
}

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

const base = (): Editor => ({
  blocks: [],
  problem: "",
  pass: false,
  staged: false,
  cost: null,
  task: null,
  result: "",
});

const followed = (primary: boolean, text: string) =>
  primary ? `Jamie ${text}.` : `Followed · ${text}`;

// ---- 7 Technology --------------------------------------------------------------------------------

/** The technology specialties of the viewer's planets: each ignores 1 prerequisite of its colour. */
const SKIPS: Partial<Record<TechColor, string>> = Object.fromEntries(
  PLANETS.filter((source) => TECH[source.id]).map((source) => [
    TECH[source.id],
    planetName(source.id),
  ]),
);
const owned = (item: Tech) => ME.tech.includes(item.name);
/** How many prerequisites of a colour the viewer has, with the technologies staged before. */
const have = (color: string, staged: string[]) =>
  TECHS.filter((item) => item.color === color && (owned(item) || staged.includes(item.id))).length +
  (SKIPS[color as TechColor] ? 1 : 0);
const lacks = (item: Tech, staged: string[]) =>
  [...new Set(item.needs)].some(
    (color) => item.needs.split(color).length - 1 > have(color, staged),
  );

function technology(state: State, primary: boolean): Editor {
  const flow = state.flow;
  const mine = flow.mine;
  const slots = primary ? 2 : 1;
  const chosen: string[] = mine.techs.slice(0, slots);
  const cell = (item: Tech): TechCellView => {
    const at = chosen.indexOf(item.id);
    // A chosen technology was checked against what was staged before it.
    const before = at >= 0 ? chosen.slice(0, at) : chosen;
    const seen: Record<string, number> = {};
    return {
      id: item.id,
      name: item.name,
      color: item.faction ? (item.color ?? undefined) : undefined,
      needs: [...item.needs].map((color) => {
        seen[color] = (seen[color] || 0) + 1;
        return {
          color: color as TechColor,
          missing: !owned(item) && seen[color] > have(color, before),
        };
      }),
      state: owned(item)
        ? "owned"
        : at >= 0 || (chosen.length < slots && !lacks(item, chosen))
          ? "open"
          : "closed",
      selected: at >= 0,
      text: item.text,
      intent: { type: "pickTech", tech: item.id },
    };
  };
  const price = primary ? ["Free", "Optional · 6 resources"] : ["4 resources"];
  const cost = primary ? (chosen.length === 2 ? 6 : 0) : chosen.length ? 4 : 0;
  const short = cost - paid(mine.pay, "res");
  const researched = names(chosen.map((id) => TECH_BY_ID[id].name));
  return {
    ...base(),
    blocks: [
      {
        kind: "techs",
        title: "Research",
        chosen: chosen.map(
          (id, index) =>
            `${TECH_BY_ID[id].name} · ${price[index].replace("Optional · ", "").toLowerCase()}`,
        ),
        aside: primary ? `${chosen.length} / 2 chosen · the second costs 6 resources` : tokenCost(),
        columns: TECH_COLORS.map(([color, name]) => ({
          color,
          label: `${name} ${have(color, []) - (SKIPS[color] ? 1 : 0)}${SKIPS[color] ? ` · +1 ${SKIPS[color]}` : ""}`,
          cells: TECHS.filter((item) => item.color === color && !item.faction).map(cell),
        })),
        bands: [
          { label: "Unit upgrades", cells: TECHS.filter((item) => !item.color).map(cell) },
          {
            label: `${ME.faction} technologies`,
            cells: TECHS.filter((item) => item.faction).map(cell),
          },
        ].filter((band) => band.cells.length),
      },
    ],
    problem:
      primary && !chosen.length
        ? "Choose 1 technology."
        : short > 0
          ? `Stage ${plural(short, "more resource")} to research.`
          : "",
    pass: !chosen.length,
    staged: chosen.length > 0,
    cost: { amount: cost, key: "res" },
    task: cost ? payTask(cost, "res", mine.pay) : null,
    result: chosen.length
      ? followed(primary, `researched ${researched}${cost ? ` for ${cost} resources` : ""}`)
      : "Passed",
  };
}

// ---- 8 Imperial ----------------------------------------------------------------------------------

function imperial(state: State, primary: boolean): Editor {
  const flow = state.flow;
  const mine = flow.mine;
  const secrets = `Secret objectives ${ME.so[1]} → ${ME.so[1] + 1}`;
  if (!primary) {
    return {
      ...base(),
      blocks: [followMenu(flow, "Draw 1 secret objective", secrets)],
      problem: mine.follow === null ? "Choose Follow or Pass." : "",
      pass: mine.follow === false,
      staged: mine.follow !== null,
      result: mine.follow ? "Followed · drew 1 secret objective" : "Passed",
    };
  }
  const scored = mine.objective ? 1 : 0;
  return {
    ...base(),
    blocks: [
      {
        kind: "menu",
        title: "Public objectives",
        rows: OBJECTIVES.map((item, index) => ({
          key: `${index + 1}`,
          title: item.name,
          text: item.text,
          state: item.state === "open" ? `Fulfilled · ${item.reason} · +1 VP` : item.reason,
          selected: mine.objective === item.name,
          disabled: item.state !== "open",
          intent: { type: "chooseOption", option: item.name, group: "objective" },
        })),
      },
      {
        kind: "card",
        title: "Then",
        aside: `Victory points ${ME.vp} → ${ME.vp + scored}`,
        rows: [
          {
            icon: "card",
            title: ["Draw 1 secret objective"],
            subtitle: `You do not control Mecatol Rex · ${secrets}`,
          },
        ],
      },
    ],
    staged: !!mine.objective,
    main: mine.objective ? undefined : "Resolve primary · score nothing",
    result: `Jamie ${mine.objective ? `scored ${mine.objective} (victory points ${ME.vp} → ${ME.vp + 1}) and drew` : "scored no objective and drew"} 1 secret objective.`,
  };
}

// ---- The frame -----------------------------------------------------------------------------------

/** The editor of the half of the card that the viewer resolves: the primary, or their secondary. */
export function strategyEditor(state: State): Editor {
  const primary = state.flow.owner === "sol";
  switch (state.flow.number ?? 1) {
    case 7:
      return technology(state, primary);
    case 8:
      return imperial(state, primary);
    default:
      return base();
  }
}

/** Esc: the last staged choice goes first. False when nothing is staged. */
function clearLast(mine: any): boolean {
  if (Object.values(mine.pay).some(Boolean)) {
    mine.pay = {};
  } else if (mine.techs.length) {
    mine.techs.pop();
  } else if (mine.objective) {
    mine.objective = null;
  } else if (mine.follow !== null) {
    mine.follow = null;
  } else {
    return false;
  }
  return true;
}

/**
 * A choice of the viewer in the open half of a strategy card. Each intent goes to the part that
 * the board serves now. Returns false when the intent is not for this card.
 */
export function strategyIntent(state: State, intent: Intent): boolean {
  const flow = state.flow;
  const mine = flow.mine;
  const editor = strategyEditor(state);
  switch (intent.type) {
    case "togglePlanet":
      if (editor.task?.target !== "planet" || !(intent.planet in editor.task.values)) {
        return true;
      }
      if (editor.task.kind === "pay") {
        mine.pay[intent.planet] = !mine.pay[intent.planet];
      }
      state.reveal = `pl:${intent.planet}`;
      return true;
    case "chooseOption": {
      const [group] = (intent.group ?? "").split(":");
      if (group === "follow") {
        const follow = intent.option === "follow";
        mine.follow = mine.follow === follow ? null : follow;
      } else if (
        group === "objective" &&
        OBJECTIVES.find((item) => item.name === intent.option)?.state === "open"
      ) {
        mine.objective = mine.objective === intent.option ? null : intent.option;
      }
      return true;
    }
    case "pickTech": {
      const at = mine.techs.indexOf(intent.tech);
      // A later technology may need the one that is removed, so it goes too.
      if (at >= 0) {
        mine.techs.splice(at);
      } else {
        const block = editor.blocks.find((item) => item.kind === "techs");
        const cells =
          block?.kind === "techs"
            ? [...block.columns.flatMap((c) => c.cells), ...block.bands.flatMap((b) => b.cells)]
            : [];
        if (cells.find((cell) => cell.id === intent.tech)?.state === "open") {
          mine.techs.push(intent.tech);
        }
      }
      if (!strategyEditor(state).cost?.amount) {
        mine.pay = {};
      }
      return true;
    }
    case "clearPart": {
      const [part, id] = intent.part.split(":");
      if (part === "tech") {
        mine.techs.splice(+id);
      }
      if (!strategyEditor(state).cost?.amount) {
        mine.pay = {};
      }
      return true;
    }
    case "clearChoice":
      return clearLast(mine);
    case "payment":
      mine.pay =
        intent.action === "auto" && editor.cost
          ? E.autoPay(editor.cost.amount, editor.cost.key)
          : {};
      return true;
    case "setPayment":
      mine.pay[intent.source] = intent.value;
      return true;
    default:
      return false;
  }
}
