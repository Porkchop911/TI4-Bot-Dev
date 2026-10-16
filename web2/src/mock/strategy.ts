// The eight strategy cards. Every card uses one frame: the parts of the card are blocks of one
// screen, the board and the player table serve the first part that is not complete, and nothing
// is sent before the main button. A secondary is the same editor as its part of the primary.
import type {
  BlockView,
  BoardTaskView,
  Intent,
  MenuRowView,
  PlayerPickView,
  RowView,
  TechCellView,
  TechColor,
  UnitType,
} from "../model";
import { plural } from "../model";
import { MAP, PAY, SEATS, TECH, U, blankMine } from "./data";
import { E } from "./loose";
import { button, openPayment } from "./select/shared";
import { TECHS, TECH_BY_ID, TECH_COLORS, type Tech } from "./tech";

type State = any;

/** The printed text of each card, shown behind hover. */
export const STRATEGY_TEXT: Record<number, { primary: string; secondary: string }> = {
  1: {
    primary: "Gain 3 command tokens. Then buy any number of tokens for 3 influence each.",
    secondary:
      "Buy command tokens. Each other player may buy tokens for 3 influence each. No strategy token is spent.",
  },
  2: {
    primary:
      "Choose 1 system other than the Mecatol Rex system that contains a planet you control; each other player places a command token from their reinforcements in that system. Then ready up to 2 exhausted planets you control.",
    secondary:
      "Spend 1 token from your strategy pool to ready up to 2 exhausted planets you control.",
  },
  3: {
    primary:
      "Choose a player other than the speaker. That player gains the speaker token. Draw 2 action cards. Look at the top 2 cards of the agenda deck. Place each card on the top or bottom of the deck in any order.",
    secondary: "Spend 1 token from your strategy pool to draw 2 action cards.",
  },
  4: {
    primary:
      "Place 1 PDS or 1 space dock on a planet you control. Place 1 PDS on a planet you control.",
    secondary:
      "Place 1 token from your strategy pool in any system; you may place either 1 space dock or 1 PDS on a planet you control in that system.",
  },
  5: {
    primary:
      "Gain 3 trade goods. Replenish commodities. Choose any number of other players. Those players use the secondary ability of this card without spending a command token.",
    secondary: "Spend 1 token from your strategy pool to replenish your commodities.",
  },
  6: {
    primary:
      "Remove 1 of your command tokens from the game board; then gain 1 command token. Redistribute any number of the command tokens on your command sheet.",
    secondary:
      "Spend 1 token from your strategy pool to use the Production ability of 1 of your space docks in your home system.",
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
const sys = (id: string) => `${MAP[id].name} · #${id}`;
const names = (list: string[]) =>
  list.length < 2 ? (list[0] ?? "") : `${list.slice(0, -1).join(", ")} and ${list.at(-1)}`;

/** Planets of the viewer that are exhausted, for Diplomacy. */
const EXHAUSTED = ["vefut", "quann", "lor"];
/** Structures of the viewer on the board. A player has 3 space docks and 6 PDS. */
const STRUCTURES: Record<string, { dock: number; pds: number }> = {
  jord: { dock: 1, pds: 1 },
  starpoint: { dock: 0, pds: 1 },
  arnor: { dock: 1, pds: 0 },
};
const STRUCTURE = { dock: ["Space dock", 3], pds: ["PDS", 6] } as const;
const AGENDAS = [
  {
    name: "Fleet Regulations",
    type: "Agenda · Law",
    text: "For: each player cannot have more than 4 tokens in their fleet pool. Against: each player places 1 command token from their reinforcements in their fleet pool.",
  },
  {
    name: "Economic Equality",
    type: "Agenda · Directive",
    text: "For: each player returns all of their trade goods to the supply. Then, each player gains 5 trade goods. Against: each player returns all of their trade goods to the supply.",
  },
];
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
/** What the Warfare secondary can produce in the home system, and the Production value there. */
const PRODUCE: UnitType[] = [
  "infantry",
  "fighter",
  "destroyer",
  "cruiser",
  "carrier",
  "dreadnought",
];
const HOME = "1";
const PRODUCTION = 6;

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

const pickTask = (target: "planet" | "system", ids: string[], chosen: string[]): BoardTaskView => ({
  target,
  kind: "pick",
  interactive: true,
  values: Object.fromEntries(ids.map((id) => [id, target === "planet" ? planetOf(id).res : 0])),
  chosen: Object.fromEntries(chosen.map((id) => [id, true])),
  verb: "Choose",
  unit: target === "planet" ? "resource" : "system",
});

/** "Change" on a chosen thing clears that part, so the board or the table serves it again. */
const change = (part: string): RowView["control"] => ({
  kind: "button",
  button: button({ type: "clearPart", part }, "Change"),
});

const planetRow = (id: string, subtitle: string, part: string): RowView => ({
  icon: "planet",
  title: [`${planetName(id)} · #${planetOf(id).system}`],
  subtitle,
  link: [`pl:${id}`, `sys:${planetOf(id).system}`],
  control: change(part),
});

/** What a secondary costs: a token of the strategy pool, or nothing when the active player gave it. */
function tokenCost(flow: any): string {
  if (flow.free?.includes("sol")) {
    return `Free · chosen by ${SEATS[flow.owner].name}`;
  }
  return `Strategy pool ${ME.tokens[2]} → ${ME.tokens[2] - 1}`;
}

/** A secondary with no choice: follow or pass, as a list. */
function followMenu(flow: any, title: string, effect: string): BlockView {
  const mine = flow.mine;
  const blocked = !ME.tokens[2] && !flow.free?.includes("sol");
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
      row("1", "follow", title, blocked ? "No strategy token" : `${effect} · ${tokenCost(flow)}`),
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
  pick: PlayerPickView | null;
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
  pick: null,
  result: "",
});

const followed = (primary: boolean, text: string) =>
  primary ? `Jamie ${text}.` : `Followed · ${text}`;

// ---- 1 Leadership --------------------------------------------------------------------------------

function leadership(state: State, primary: boolean): Editor {
  const own = state.flow.mine;
  const free: number = E.flowFree(state);
  const total = free + own.buy;
  const placed = own.pools.t + own.pools.f + own.pools.s;
  const pools: RowView[] = (
    [
      ["t", "Tactic pool", 0],
      ["f", "Fleet pool", 1],
      ["s", "Strategy pool", 2],
    ] as const
  ).map(([key, name, index]) => ({
    title: [name],
    tokens: { now: ME.tokens[index], after: ME.tokens[index] + own.pools[key], up: key === "f" },
    control: {
      kind: "counter",
      counter: {
        id: `pools.${key}`,
        value: own.pools[key],
        max: own.pools[key] + Math.max(0, total - placed),
        label: name.toLowerCase() + " token",
      },
    },
  }));
  return {
    ...base(),
    blocks: [
      {
        kind: "card",
        title: "Buy command tokens",
        aside: `${own.buy} bought · ${own.buy * 3} influence`,
        rows: [
          {
            icon: "token",
            title: ["Command tokens"],
            subtitle: "3 influence each",
            control: {
              kind: "counter",
              counter: { id: "buy", value: own.buy, max: 3, label: "bought token" },
            },
          },
        ],
      },
      {
        kind: "card",
        title: "Place in your pools",
        aside: `${free ? `${free} gained + ` : ""}${own.buy} bought · ${placed} / ${total} placed`,
        bad: placed !== total,
        rows: pools,
      },
    ],
    problem: E.flowProblem(state),
    staged: own.buy > 0 || placed > 0,
    pass: !own.buy,
    cost: { amount: own.buy * 3, key: "inf" },
    task: own.buy ? payTask(own.buy * 3, "inf", own.pay) : null,
    result: primary
      ? `Jamie gained 3 command tokens${own.buy ? ` and bought ${own.buy} for ${own.buy * 3} influence` : ""}.`
      : own.buy
        ? `Followed · bought ${plural(own.buy, "token")} for ${own.buy * 3} influence`
        : "Passed",
  };
}

// ---- 2 Diplomacy ---------------------------------------------------------------------------------

function diplomacy(state: State, primary: boolean, seats: string[]): Editor {
  const flow = state.flow;
  const mine = flow.mine;
  const others = seats.filter((seat) => seat !== "sol").map((seat) => SEATS[seat].name);
  const who =
    others.length > 3
      ? `${others.slice(0, 2).join(", ")} and ${others.length - 2} more`
      : names(others);
  const systems = [...new Set(PLANETS.map((source) => source.system!))].filter((id) => id !== "50");
  const ready: BlockView = {
    kind: "card",
    title: "Ready planets",
    aside: `${mine.planets.length} / 2 · ${primary ? "choose on the map" : tokenCost(flow)}`,
    rows: mine.planets.length
      ? mine.planets.map((id: string) =>
          planetRow(
            id,
            `Exhausted → ready · ${plural(planetOf(id).res, "resource")} / ${planetOf(id).inf} influence`,
            `planet:${id}`,
          ),
        )
      : [{ icon: "planet", title: ["No planet chosen"] }],
  };
  const planets = pickTask("planet", EXHAUSTED, mine.planets);
  const readied = mine.planets.length
    ? `readied ${names(mine.planets.map(planetName))}`
    : "readied no planet";
  if (!primary) {
    return {
      ...base(),
      blocks: [ready],
      pass: !mine.planets.length,
      staged: mine.planets.length > 0,
      task: planets,
      result: mine.planets.length ? `Followed · ${readied}` : "Passed",
    };
  }
  return {
    ...base(),
    blocks: [
      {
        kind: "card",
        title: "System",
        aside: "Choose on the map",
        rows: mine.system
          ? [
              {
                icon: "token",
                title: [sys(mine.system)],
                subtitle: `${who} place a command token here`,
                link: [`sys:${mine.system}`],
                control: change("system"),
              },
            ]
          : [{ icon: "token", title: ["No system chosen"] }],
      },
      ready,
    ],
    problem: mine.system ? "" : "Choose 1 system.",
    staged: !!mine.system,
    task: mine.system ? planets : pickTask("system", systems, []),
    result: mine.system
      ? `Jamie chose ${sys(mine.system)}: each other player placed a command token there. Jamie ${readied}.`
      : "",
  };
}

// ---- 3 Politics ----------------------------------------------------------------------------------

function politics(state: State, primary: boolean, seats: string[]): Editor {
  const flow = state.flow;
  const mine = flow.mine;
  const draw = `Action cards ${ME.ac} → ${ME.ac + 2}`;
  if (!primary) {
    return {
      ...base(),
      blocks: [followMenu(flow, "Draw 2 action cards", draw)],
      problem: mine.follow === null ? "Choose Follow or Pass." : "",
      pass: mine.follow === false,
      staged: mine.follow !== null,
      result: mine.follow ? "Followed · drew 2 action cards" : "Passed",
    };
  }
  const speaker = seats.find((seat) => SEATS[seat].speaker) ?? seats[0];
  if (!flow.sent) {
    return {
      ...base(),
      blocks: [
        {
          kind: "card",
          title: "Speaker",
          aside: "Choose in the player table",
          rows: mine.seat
            ? [
                {
                  title: [{ seat: mine.seat }, ` ${SEATS[mine.seat].name}`],
                  subtitle: `Speaker: ${SEATS[speaker].name} → ${SEATS[mine.seat].name}`,
                  control: change("seat"),
                },
              ]
            : [{ title: ["No player chosen"] }],
        },
        {
          kind: "card",
          title: "Action cards",
          rows: [{ icon: "card", title: ["Draw 2 action cards"], subtitle: draw }],
        },
      ],
      problem: mine.seat ? "" : "Choose 1 player.",
      staged: !!mine.seat,
      main: "Resolve · look at the agenda deck",
      pick: {
        verb: "Choose",
        seats: Object.fromEntries(
          seats.map((seat) => [seat, { reason: seat === speaker ? "Is the speaker" : null }]),
        ),
        chosen: mine.seat ? [mine.seat] : [],
      },
    };
  }
  // The agenda cards are revealed to the viewer only: where each one goes, and the deck after.
  const place = (where: "top" | "bottom") => {
    const at = [0, 1].filter((index) => mine.agenda[index] === where);
    return mine.swap ? at.reverse() : at;
  };
  const tops = place("top");
  const bottoms = place("bottom");
  const both = tops.length === 2 || bottoms.length === 2;
  const swap: RowView["control"] = {
    kind: "button",
    button: button({ type: "chooseOption", option: "swap", group: "agenda" }, "Swap"),
  };
  const deck: RowView[] = [
    ...tops.map((index, at) => ({
      icon: "card" as const,
      title: [at ? "Second from the top" : "Top of the deck · the next agenda"],
      subtitle: AGENDAS[index].name,
      control: both && !at ? swap : undefined,
    })),
    ...bottoms.map((index, at) => ({
      icon: "card" as const,
      title: [at === bottoms.length - 1 ? "Bottom of the deck" : "Second from the bottom"],
      subtitle: AGENDAS[index].name,
      control: both && !at ? swap : undefined,
    })),
  ];
  const where = (index: number) =>
    `${AGENDAS[index].name} ${mine.agenda[index] === "top" ? "on top" : "on the bottom"}`;
  return {
    ...base(),
    blocks: [
      ...AGENDAS.flatMap((agenda, index): BlockView[] => [
        { kind: "source", ...agenda },
        {
          kind: "menu",
          rows: (["top", "bottom"] as const).map((option, at) => ({
            key: `${index * 2 + at + 1}`,
            title: option === "top" ? "Top of the deck" : "Bottom of the deck",
            selected: mine.agenda[index] === option,
            disabled: false,
            intent: { type: "chooseOption", option, group: `agenda:${index}` },
          })),
        },
      ]),
      {
        kind: "card",
        title: "Agenda deck after",
        aside: `${tops.length + bottoms.length} / 2 placed`,
        rows: deck.length ? deck : [{ icon: "card", title: ["No card placed"] }],
      },
    ],
    problem: mine.agenda.includes(null) ? "Place both agenda cards." : "",
    staged: mine.agenda.some(Boolean),
    result: `Jamie made ${SEATS[flow.speaker]?.name} the speaker, drew 2 action cards and placed ${where(0)} and ${where(1)} of the agenda deck.`,
  };
}

// ---- 4 Construction ------------------------------------------------------------------------------

function construction(state: State, primary: boolean): Editor {
  const flow = state.flow;
  const mine = flow.mine;
  const slots: { type: "dock" | "pds" | null; planet: string | null }[] = primary
    ? mine.build
    : mine.build.slice(0, 1);
  const has = (id: string) => STRUCTURES[id] ?? { dock: 0, pds: 0 };
  const staged = (type: string, planet?: string) =>
    slots.filter((slot) => slot.planet && slot.type === type && (!planet || slot.planet === planet))
      .length;
  const onBoard = (type: "dock" | "pds") =>
    Object.values(STRUCTURES).reduce((sum, item) => sum + item[type], 0) + staged(type);
  const open = slots.findIndex((slot) => !slot.planet);
  const type = open >= 0 ? slots[open].type : null;
  const allowed = PLANETS.filter((source) =>
    type === "dock"
      ? !has(source.id).dock && !staged("dock", source.id) && onBoard("dock") < 3
      : has(source.id).pds + staged("pds", source.id) < 2 && onBoard("pds") < 6,
  ).map((source) => source.id);
  const label = ["1 of 2 · space dock or PDS", "2 of 2 · PDS only"];
  const rows: RowView[] = slots.map((slot, index) => {
    if (!slot.planet) {
      return {
        icon: "planet",
        title: [primary ? label[index] : "Space dock or PDS"],
        subtitle: slot.type ? "No planet chosen" : "Choose the structure first",
      };
    }
    const here = has(slot.planet);
    return planetRow(
      slot.planet,
      `${STRUCTURE[slot.type!][0]} · ${planetName(slot.planet)} has ${plural(here.dock, "space dock")} and ${here.pds} PDS${primary ? "" : ` · your command token goes to #${planetOf(slot.planet).system}`}`,
      `build:${index}`,
    );
  });
  const built = slots.flatMap((slot) => (slot.planet ? [slot.planet] : []));
  const placed = slots
    .filter((slot) => slot.planet)
    .map(
      (slot) =>
        `a ${STRUCTURE[slot.type!][0].replace("Space", "space")} on ${planetName(slot.planet!)}`,
    );
  return {
    ...base(),
    blocks: [
      {
        kind: "gauges",
        gauges: (["dock", "pds"] as const).map((item) => ({
          label: `${STRUCTURE[item][0]}s on the board`.replace("PDSs", "PDS"),
          used: onBoard(item),
          total: STRUCTURE[item][1],
        })),
      },
      {
        kind: "menu",
        title: primary ? "Structure 1 of 2" : "Structure",
        rows: (["dock", "pds"] as const).map((item, index) => ({
          key: `${index + 1}`,
          title: STRUCTURE[item][0],
          state: `${STRUCTURE[item][1] - onBoard(item)} in your reinforcements`,
          selected: slots[0].type === item,
          disabled: !!slots[0].planet,
          intent: { type: "chooseOption", option: item, group: "structure" },
        })),
      },
      {
        kind: "card",
        title: primary ? "Structures" : "Planet",
        aside: primary ? "Choose on the map" : `Choose on the map · ${tokenCost(flow)}`,
        rows,
      },
    ],
    problem: primary && !placed.length ? "Place at least 1 structure." : "",
    pass: !placed.length,
    staged: !!slots[0].type || placed.length > 0,
    main: primary && placed.length === 1 ? "Resolve primary · 1 structure" : undefined,
    // The chosen planets stay marked on the board, also when no structure is left to place.
    task:
      open >= 0 && type
        ? pickTask("planet", allowed, built)
        : built.length
          ? { ...pickTask("planet", [], built), interactive: false }
          : null,
    result: placed.length ? followed(primary, `placed ${names(placed)}`) : "Passed",
  };
}

// ---- 5 Trade -------------------------------------------------------------------------------------

function trade(state: State, primary: boolean, seats: string[]): Editor {
  const flow = state.flow;
  const mine = flow.mine;
  const comm = (seat: string) => `Commodities ${SEATS[seat].comm[0]} → ${SEATS[seat].comm[1]}`;
  if (!primary) {
    return {
      ...base(),
      blocks: [followMenu(flow, "Replenish commodities", comm("sol"))],
      problem: mine.follow === null ? "Choose Follow or Pass." : "",
      pass: mine.follow === false,
      staged: mine.follow !== null,
      result: mine.follow ? "Followed · replenished commodities" : "Passed",
    };
  }
  return {
    ...base(),
    blocks: [
      {
        kind: "card",
        title: "Trade",
        rows: [
          {
            title: ["Gain 3 trade goods"],
            subtitle: `Trade goods ${ME.tg} → ${ME.tg + 3}`,
          },
          { title: ["Replenish commodities"], subtitle: comm("sol") },
        ],
      },
      {
        kind: "card",
        title: "Free replenish",
        aside: `${mine.seats.length} chosen · choose in the player table`,
        rows: mine.seats.length
          ? mine.seats.map((seat: string) => ({
              title: [{ seat }, ` ${SEATS[seat].name}`],
              subtitle: `${comm(seat)} · no strategy token`,
              control: change(`seat:${seat}`),
            }))
          : [{ title: ["No player chosen"] }],
      },
    ],
    staged: mine.seats.length > 0,
    pick: {
      verb: "Choose",
      seats: Object.fromEntries(
        seats.map((seat) => {
          const full = SEATS[seat].comm[0] >= SEATS[seat].comm[1];
          const gain = SEATS[seat].comm[1] - SEATS[seat].comm[0];
          return [
            seat,
            {
              reason: seat === "sol" ? "You" : full ? "Commodities full" : null,
              note: `+${plural(gain, "commodity", "commodities")}`,
            },
          ];
        }),
      ),
      chosen: mine.seats,
    },
    result: `Jamie gained 3 trade goods, replenished commodities and chose ${
      mine.seats.length
        ? `${names(mine.seats.map((seat: string) => SEATS[seat].name))} for a free replenish`
        : "no player"
    }.`,
  };
}

// ---- 6 Warfare -----------------------------------------------------------------------------------

function warfare(state: State, primary: boolean): Editor {
  const flow = state.flow;
  const mine = flow.mine;
  if (!primary) {
    const count = (type: UnitType): number => mine.counts[type] || 0;
    const units = PRODUCE.reduce((sum, type) => sum + count(type), 0);
    const cost = PRODUCE.reduce(
      (sum, type) => sum + Math.ceil(count(type) / (U[type].per || 1)) * U[type].cost!,
      0,
    );
    const short = cost - paid(mine.pay, "res");
    const made = PRODUCE.filter(count).map(
      (type) => `${count(type)} ${E.unitName(type, count(type)).toLowerCase()}`,
    );
    return {
      ...base(),
      blocks: [
        {
          kind: "gauges",
          gauges: [{ label: `Production in ${sys(HOME)}`, used: units, total: PRODUCTION }],
        },
        {
          kind: "card",
          title: "Produce",
          aside: `${plural(units, "unit")} · ${plural(cost, "resource")} · ${tokenCost(flow)}`,
          bad: units > PRODUCTION,
          rows: PRODUCE.map((type) => ({
            title: [E.unitName(type, 2)],
            subtitle: `Cost ${U[type].cost}${U[type].per ? ` for ${U[type].per}` : ""}`,
            link: [`sys:${HOME}`],
            control: {
              kind: "counter",
              counter: {
                id: `counts.${type}`,
                value: count(type),
                max: 9,
                label: E.unitName(type).toLowerCase(),
              },
            },
          })),
        },
      ],
      problem:
        units > PRODUCTION
          ? `The Production value is ${PRODUCTION}. Remove ${plural(units - PRODUCTION, "unit")}.`
          : short > 0
            ? `Stage ${plural(short, "more resource")} to produce.`
            : "",
      staged: units > 0,
      pass: !units,
      cost: { amount: cost, key: "res" },
      task: cost ? payTask(cost, "res", mine.pay) : null,
      result: units
        ? `Followed · produced ${names(made)} in ${MAP[HOME].name} for ${plural(cost, "resource")}`
        : "Passed",
    };
  }
  const now: number[] = ME.tokens;
  const total = now[0] + now[1] + now[2] + 1;
  const placed = mine.pools.t + mine.pools.f + mine.pools.s;
  const tokens = Object.keys(MAP).filter((id) => MAP[id].token);
  return {
    ...base(),
    blocks: [
      {
        kind: "card",
        title: "Command token",
        aside: "Choose on the map",
        rows: mine.system
          ? [
              {
                icon: "token",
                title: [sys(mine.system)],
                subtitle: "The token returns to your reinforcements · you gain 1 command token",
                link: [`sys:${mine.system}`],
                control: change("system"),
              },
            ]
          : [{ icon: "token", title: ["No system chosen"] }],
      },
      {
        kind: "card",
        title: "Command pools",
        aside: `${placed} / ${total} placed`,
        bad: placed !== total,
        rows: (
          [
            ["t", "Tactic pool", 0],
            ["f", "Fleet pool", 1],
            ["s", "Strategy pool", 2],
          ] as const
        ).map(([key, name, index]) => ({
          title: [name],
          tokens: { now: now[index], after: mine.pools[key], up: key === "f" },
          control: {
            kind: "counter",
            counter: {
              id: `pools.${key}`,
              value: mine.pools[key],
              max: mine.pools[key] + Math.max(0, total - placed),
              label: name.toLowerCase() + " token",
            },
          },
        })),
      },
    ],
    problem: !mine.system
      ? "Choose 1 system with your command token."
      : placed !== total
        ? `Place ${total} tokens in your pools. ${placed} placed.`
        : "",
    staged: !!mine.system || poolsChanged(mine, 6),
    task: pickTask("system", tokens, mine.system ? [mine.system] : []),
    result: mine.system
      ? `Jamie removed the command token from ${sys(mine.system)} and gained 1 command token. Pools ${now.join(" · ")} → ${mine.pools.t} · ${mine.pools.f} · ${mine.pools.s}.`
      : "",
  };
}

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
        aside: primary
          ? `${chosen.length} / 2 chosen · the second costs 6 resources`
          : tokenCost(flow),
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
export function strategyEditor(state: State, seats: string[]): Editor {
  const primary = state.flow.owner === "sol";
  switch (state.flow.number ?? 1) {
    case 2:
      return diplomacy(state, primary, seats);
    case 3:
      return politics(state, primary, seats);
    case 4:
      return construction(state, primary);
    case 5:
      return trade(state, primary, seats);
    case 6:
      return warfare(state, primary);
    case 7:
      return technology(state, primary);
    case 8:
      return imperial(state, primary);
    default:
      return leadership(state, primary);
  }
}

/** The pool counters differ from where the card starts them. */
const poolsChanged = (mine: any, number: number) => {
  const blank = blankMine(number).pools;
  return (["t", "f", "s"] as const).some((key) => mine.pools[key] !== blank[key]);
};

/** Esc: the last staged choice goes first. False when nothing is staged. */
function clearLast(mine: any, number: number): boolean {
  if (Object.values(mine.pay).some(Boolean)) {
    mine.pay = {};
  } else if (poolsChanged(mine, number)) {
    mine.pools = blankMine(number).pools;
  } else if (mine.buy) {
    mine.buy = 0;
  } else if (Object.values(mine.counts).some(Boolean)) {
    mine.counts = {};
  } else if (mine.techs.length) {
    mine.techs.pop();
  } else if (mine.planets.length) {
    mine.planets.pop();
  } else if (mine.build.some((slot: any) => slot.planet)) {
    mine.build.findLast((slot: any) => slot.planet).planet = null;
  } else if (mine.build[0].type) {
    mine.build[0].type = null;
  } else if (mine.agenda.some(Boolean)) {
    Object.assign(mine, { agenda: [null, null], swap: false });
  } else if (mine.objective) {
    mine.objective = null;
  } else if (mine.seats.length) {
    mine.seats.pop();
  } else if (mine.seat) {
    mine.seat = null;
  } else if (mine.system) {
    mine.system = null;
  } else if (mine.follow !== null) {
    mine.follow = null;
  } else {
    return false;
  }
  return true;
}

const toggle = (list: string[], id: string, max = Infinity) => {
  if (list.includes(id)) {
    list.splice(list.indexOf(id), 1);
  } else if (list.length < max) {
    list.push(id);
  }
};

/**
 * A choice of the viewer in the open half of a strategy card. Each intent goes to the part that
 * the board or the player table serves now. Returns false when the intent is not for this card.
 */
export function strategyIntent(state: State, seats: string[], intent: Intent): boolean {
  const flow = state.flow;
  const mine = flow.mine;
  const number = flow.number ?? 1;
  const primary = flow.owner === "sol";
  const editor = strategyEditor(state, seats);
  switch (intent.type) {
    case "toggleSystem":
      if (editor.task?.target !== "system" || !(intent.system in editor.task.values)) {
        return true;
      }
      mine.system = mine.system === intent.system ? null : intent.system;
      state.reveal = `sys:${intent.system}`;
      return true;
    case "togglePlanet":
      if (editor.task?.target !== "planet" || !(intent.planet in editor.task.values)) {
        return true;
      }
      if (editor.task.kind === "pay") {
        mine.pay[intent.planet] = !mine.pay[intent.planet];
      } else if (number === 2) {
        toggle(mine.planets, intent.planet, 2);
      } else if (number === 4) {
        const slots = primary ? mine.build : mine.build.slice(0, 1);
        const open = slots.find((slot: any) => !slot.planet);
        if (open) {
          open.planet = intent.planet;
        }
      }
      state.reveal = `pl:${intent.planet}`;
      return true;
    case "pickSeat":
      if (editor.pick?.seats[intent.seat]?.reason !== null) {
        return true;
      }
      if (number === 5) {
        toggle(mine.seats, intent.seat);
      } else {
        mine.seat = mine.seat === intent.seat ? null : intent.seat;
      }
      return true;
    case "chooseOption": {
      const [group, index] = (intent.group ?? "").split(":");
      if (group === "follow") {
        const follow = intent.option === "follow";
        mine.follow = mine.follow === follow ? null : follow;
      } else if (group === "structure") {
        if (!mine.build[0].planet) {
          mine.build[0].type = mine.build[0].type === intent.option ? null : intent.option;
        }
      } else if (group === "agenda" && index === undefined) {
        mine.swap = !mine.swap;
      } else if (group === "agenda") {
        mine.agenda[+index] = mine.agenda[+index] === intent.option ? null : intent.option;
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
      if (!strategyEditor(state, seats).cost?.amount) {
        mine.pay = {};
      }
      return true;
    }
    case "clearPart": {
      const [part, id] = intent.part.split(":");
      if (part === "system") {
        mine.system = null;
      } else if (part === "planet") {
        toggle(mine.planets, id);
      } else if (part === "seat" && id) {
        toggle(mine.seats, id);
      } else if (part === "seat") {
        mine.seat = null;
      } else if (part === "build") {
        mine.build[+id].planet = null;
      } else if (part === "tech") {
        mine.techs.splice(+id);
      }
      if (!strategyEditor(state, seats).cost?.amount) {
        mine.pay = {};
      }
      return true;
    }
    case "clearChoice":
      return clearLast(mine, number);
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
