// The whole screen: toolbar, player table, board, action panel, reference drawers.
import type {
  ActionCardView,
  ComponentCardView,
  DeckSecretView,
  ExplorationDeck,
  ExplorationDeckView,
  ListSectionView,
  LogEntryView,
  PlayerTableView,
  ReferenceView,
  ScoreCellView,
  ScoreView,
  Rich,
  ShellView,
  ToolbarView,
} from "../../model";
import {
  ACTION_CARDS,
  AGENDA_GROUPS,
  AGENDAS,
  CARD_GROUPS,
  CARDS,
  DISCARDED_CARDS,
  HISTORY,
  REACTION_CARDS,
  SCORE,
  SEATS,
  SECRET_GROUPS,
  SECRETS,
} from "../data";
import { E } from "../loose";
import { OBJECTIVES, strategyEditor } from "../strategy";
import {
  currentState,
  decisionShape,
  pastState,
  seatReason,
  type State,
  type World,
} from "../world";
import { selectBoard } from "./board";
import { selectFlow } from "./flows";
import { isTactical, seatViews, stagesOf, statusText, summaryOf } from "./shared";
import { applyItems, selectTactical } from "./tactical";

const actor = (state: State): string => state.flow?.owner || "sol";

/** Who holds each strategy card, by index (card number − 1). Free cards have no owner. */
function cardOwners(world: World, state: State) {
  const table = world.table;
  if (table.cards) {
    return CARDS.map((_, index) => ({
      owner: table.seats.find((seat) => table.cards![seat]?.includes(index + 1)) ?? "",
      used: world.usedCards.includes(index + 1),
      tg: 0,
    }));
  }
  const owners = (
    [
      ["sol"],
      ["xxcha", true],
      ["letnev"],
      ["jolnar", true],
      ["hacan"],
      ["mentak", true],
      ["yssaril", true],
      ["naalu"],
    ] as [string, boolean?, number?][]
  ).map(([owner, used, tg]) => ({ owner, used: !!used, tg: tg ?? 0 }));
  if (state.kind === "strategic") {
    // The open card is with the player who plays it; that player's card goes to the other seat.
    const at = (state.flow.number ?? 1) - 1;
    const other = owners.findIndex((item) => item.owner === state.flow.owner);
    [owners[at].owner, owners[other].owner] = [owners[other].owner, owners[at].owner];
    owners[other].used = owners[at].used;
    owners[at].used = true;
  }
  for (const number of world.usedCards) {
    owners[number - 1].used = true;
  }
  return owners;
}

function toolbar(world: World, state: State): ToolbarView {
  const draft = state.mode === "draft" && world.mode === "draft";
  return {
    round: 3,
    turn: 2,
    phase: "Action phase",
    status: statusText(world, state),
    workspace: world.mode,
    draftLocked: world.viewer !== "sol" ? "A draft is private to the acting player" : null,
    past: world.workspaces.history ? { label: world.workspaces.history.past.label } : null,
    draft: draft
      ? {
          canUndo: state.cursor > 0,
          canRedo: state.cursor < state.history.length - 1,
          canApply: E.canApply(state) && world.viewer === "sol" && !E.pendingEdit(state),
          applyHint: E.applyReadiness(state),
        }
      : null,
  };
}

/** The relic fragments of the demo. */
/** A card of the demo: no timing, no state, nothing to unlock. */
const CARD: ComponentCardView = {
  name: "",
  kind: "",
  window: "",
  text: "",
  state: null,
  unlock: "",
};
const SHARD =
  "When you gain this card or lose this card, gain 1 victory point or lose 1 victory point.";
const FRAGMENTS: Record<string, { deck: ExplorationDeck; count: number }[]> = {
  sol: [
    { deck: "cultural", count: 2 },
    { deck: "hazardous", count: 1 },
  ],
  hacan: [{ deck: "industrial", count: 1 }],
};

const card = (
  name: string,
  kind: string,
  text: string,
  left: number,
  total: number,
  drawn: string[] = [],
) => ({ name, kind, text, left, total, drawn });
/** Two decks of the demo, short: the sheet is the same for every deck. */
const EXPLORATION: Omit<ExplorationDeckView, "fragments">[] = [
  {
    deck: "cultural",
    title: "Cultural",
    left: 5,
    total: 8,
    cards: [
      card(
        "Cultural Relic Fragment",
        "Fragment",
        "ACTION: Purge 3 of your cultural relic fragments to gain 1 relic.",
        2,
        4,
        ["Round 1 · Sol · Lodor", "Round 2 · Sol · Corneeq"],
      ),
      card(
        "Dyson Sphere",
        "Attach",
        "This planet's resource value is increased by 2 and its influence value is increased by 1.",
        0,
        1,
        ["Round 2 · Sol · Quann"],
      ),
      card(
        "Freelancers",
        "Instant",
        "You may produce 1 unit in this system; you may spend influence as if it were resources to produce this unit.",
        2,
        2,
      ),
      card("Paradise World", "Attach", "This planet's influence value is increased by 2.", 1, 1),
    ],
  },
  {
    deck: "hazardous",
    title: "Hazardous",
    left: 4,
    total: 6,
    cards: [
      card(
        "Core Mine",
        "Instant",
        "If you have at least 1 mech on this planet, or if you remove 1 infantry from this planet, gain 1 trade good.",
        2,
        2,
      ),
      card(
        "Hazardous Relic Fragment",
        "Fragment",
        "ACTION: Purge 3 of your hazardous relic fragments to gain 1 relic.",
        2,
        3,
        ["Round 2 · Sol · Starpoint"],
      ),
      card(
        "Warfare Research Facility",
        "Attach",
        "This planet has a red technology specialty. If this planet already has a technology specialty, this planet's resource and influence values are each increased by 1 instead.",
        0,
        1,
        ["Round 1 · Sol · Vefut"],
      ),
    ],
  },
];

/** One row for each player, in seat order. Rows do not move. */
/** The Objectives sheet: every source of a point of the demo against every seat. */
function score(table: string[], viewer: string): ScoreView {
  // The viewer is first, then the seats by their points.
  const seats = [...table].sort(
    (a, b) => Number(b === viewer) - Number(a === viewer) || SEATS[b].vp - SEATS[a].vp,
  );
  const objective = (item: { name: string; text: string; scored: string[] }, worth: number) => ({
    name: item.name,
    text: item.text,
    worth,
    open: item.scored.length < seats.length,
    cells: seats.map((seat): ScoreCellView => ({
      kind: item.scored.includes(seat) ? "scored" : "none",
    })),
  });
  const points = (
    name: string,
    of: Record<string, number>,
    from: Record<string, string[]> = {},
  ) => ({
    name,
    text: "",
    worth: null,
    // A note changes hands, a law can be repealed, and the custodians are there once.
    open:
      name === "Support for the Throne" ||
      name.startsWith("Agenda · ") ||
      (name === "Custodians of Mecatol Rex" && Object.keys(of).length === 0),
    cells: seats.map((seat): ScoreCellView =>
      of[seat]
        ? { kind: "points", value: of[seat], from: from[seat] ?? [], detail: [] }
        : { kind: "none" },
    ),
  });
  const stage = (worth: number) =>
    SCORE.public.filter((item) => item.worth === worth).map((item) => objective(item, worth));
  return {
    target: 10,
    seats: seats.map((seat) => ({ seat, points: SEATS[seat].vp, isMe: seat === viewer })),
    groups: [
      {
        title: "Public objectives · stage I",
        rows: [...OBJECTIVES.map((item) => objective(item, 1)), ...stage(1)],
      },
      { title: "Public objectives · stage II", rows: stage(2) },
      {
        title: "Other points",
        rows: [
          points(
            "Secret objectives",
            Object.fromEntries(seats.map((seat) => [seat, SEATS[seat].so[0]])),
          ),
          ...SCORE.other.map((item) => points(item.name, item.points, item.from)),
        ],
      },
    ],
    // The demo has no catalogue: the deck is the revealed objectives only.
    deck: [],
  };
}

function players(world: World, state: State): PlayerTableView {
  const owners = cardOwners(world, state);
  const seats = world.table.seats;
  const now = world.mode === "history" ? null : actor(state);
  // The lowest card of a seat gives its place in the initiative order.
  const card = (seat: string) => owners.findIndex((item) => item.owner === seat);
  const queue = seats
    .filter((seat) => !SEATS[seat].passed && card(seat) >= 0)
    .sort((a, b) => card(a) - card(b));
  const next = now && queue.length > 1 ? queue[(queue.indexOf(now) + 1) % queue.length] : null;
  const me = world.viewer === "observer" ? "sol" : world.viewer;
  const picking = world.mode === "live" && decisionShape(state) === "seat";
  return {
    pick:
      world.mode === "live" && state.kind === "strategic" && E.flowEditing(state)
        ? strategyEditor(state, seats).pick
        : picking
          ? {
              verb: "Choose",
              seats: Object.fromEntries(
                seats.map((seat) => [seat, { reason: seatReason(world, seat) }]),
              ),
              chosen: state.flow.chosen ? [state.flow.chosen] : [],
            }
          : null,
    rows: seats.map((seat) => {
      const player = SEATS[seat];
      const tokens =
        seat === "sol" && isTactical(state) && state.mode === "live" && state.done[0]
          ? [player.tokens[0] - 1, ...player.tokens.slice(1)]
          : player.tokens;
      return {
        seat,
        isMe: seat === me,
        turn: now === seat ? "now" : next === seat ? "next" : null,
        speaker: !!player.speaker,
        passed: !!player.passed,
        victoryPoints: player.vp,
        strategyCards: owners.flatMap((item, index) =>
          item.owner === seat ? [{ number: index + 1, name: CARDS[index], used: item.used }] : [],
        ),
        resources: player.res,
        influence: player.inf,
        tradeGoods: player.tg,
        commodities: player.comm,
        tokens,
        actionCards: player.ac,
        secrets: player.so,
        planets: player.planets,
        abilities: (player.abilities as string[]).map((line) => {
          const [name, text] = line.split(": ");
          return { ...CARD, name, kind: "Faction ability", text };
        }),
        technologies: player.tech,
        // "Agent ready · Commander unlocked · Hero locked"
        leaders: (player.leaders as string).split(" · ").map((leader) => {
          const [kind, state] = leader.split(" ");
          return {
            ...CARD,
            name: kind,
            kind,
            state: state === "unlocked" ? "ready" : (state as ComponentCardView["state"]),
          };
        }),
        components:
          seat === "hacan"
            ? [{ ...CARD, name: "Shard of the Throne", kind: "Relic", text: SHARD }]
            : [],
        fragments: FRAGMENTS[seat] ?? [],
      };
    }),
    freeCards: owners.flatMap((item, index) =>
      item.owner ? [] : [{ number: index + 1, name: CARDS[index], tradeGoods: item.tg }],
    ),
  };
}

function reference(world: World, state: State): ReferenceView {
  // The hand of the viewer. The demo has hands for the two sides of the battle only.
  const hand: string[] = state.hands[world.viewer] ?? [];
  const sym = (seat: string) => ({ seat });
  const section = (title: string, rows: (Rich | string)[]): ListSectionView => ({
    title,
    rows: rows.map((row) => (typeof row === "string" ? [row] : row)),
  });
  // A card of a reaction window is printed with its timing first.
  const printed = (name: string): ActionCardView => {
    const reaction = REACTION_CARDS[name];
    const [window, ...rest] = (reaction ?? "").split(": ");
    return reaction
      ? {
          name,
          window: `${window}:`,
          text: rest.join(": "),
          action: false,
          group: CARD_GROUPS[name] ?? "Other",
        }
      : { name, window: "ACTION:", text: ACTION_CARDS[name] ?? "", action: true, group: "Action" };
  };
  const held = [...(E.own() ? world.table.actionCards : []), ...hand];
  const count = (cards: string[], name: string) => cards.filter((card) => card === name).length;
  const names = [...Object.keys(REACTION_CARDS), ...Object.keys(ACTION_CARDS)].sort();
  const groups = ["Action", ...new Set(Object.values(CARD_GROUPS))];
  // A card that is held is not faked, and a component action has nothing to fake.
  const canFake = (name: string) => !held.includes(name) && !printed(name).action;
  const copies = (name: string) => (REACTION_CARDS[name] ? 4 : 1);
  const total = names.reduce((sum, name) => sum + copies(name), 0);
  // A hand is seen by its holder only; what is scored is seen by everyone.
  const secret = (item: (typeof SECRETS)[number]): DeckSecretView => {
    const by = item.scored ?? (item.held === world.viewer ? item.held : null);
    return {
      name: item.name,
      phase: `${item.phase} phase`,
      text: item.text,
      group: item.group,
      state: item.scored ? "scored" : by ? "held" : "unseen",
      by,
      mine: by === world.viewer,
    };
  };
  const secrets = SECRETS.map(secret);
  const mine = secrets.filter((item) => item.mine);
  const live = world.workspaces.live;
  const seats = world.table.seats;
  const entry = (item: (typeof HISTORY)[number]): LogEntryView => {
    const past = pastState(item);
    return {
      id: item.id,
      seat: item.seat,
      type: `${item.type}${isTactical(past) ? "" : " · " + past.flow.card}`,
      summary: summaryOf(past),
      stages: stagesOf(past).map(([name, text]) => ({ name, text })),
      current: false,
    };
  };
  const current: LogEntryView[] =
    live.kind !== "picker" && live.kind !== "waiting"
      ? [
          {
            id: "current",
            seat: actor(live),
            type: `${isTactical(live) ? "Tactical" : live.kind === "strategic" ? "Strategic" : "Component"} · in progress`,
            summary: summaryOf(live) || "Not started",
            stages: [],
            current: true,
          },
        ]
      : [];
  return {
    objectives: score(seats, world.viewer),
    technology: seats.map((id) =>
      section(`${SEATS[id].name} · ${SEATS[id].faction}`, SEATS[id].tech),
    ),
    cards: [
      section(
        "Strategy cards",
        cardOwners(world, state).map((card, index): Rich =>
          card.owner
            ? [
                `${index + 1} ${CARDS[index]} · `,
                sym(card.owner),
                ` ${SEATS[card.owner].name}${card.used ? " · used" : ""}`,
              ]
            : [`${index + 1} ${CARDS[index]} · unclaimed · ${card.tg} TG`],
        ),
      ),
      section("Promissory notes", [
        "Trade Agreement (Hacan) · held by you",
        "Ceasefire (Sol) · held by Blair",
      ]),
    ],
    actionCards: {
      held: held.length,
      limit: 7,
      deck: {
        left: total - DISCARDED_CARDS.length - held.length - 5,
        total,
        discarded: DISCARDED_CARDS.length,
      },
      hand: [...new Set(held)]
        .sort(
          (a, b) =>
            groups.indexOf(printed(a).group) - groups.indexOf(printed(b).group) ||
            a.localeCompare(b),
        )
        .map((name) => ({
          ...printed(name),
          copies: count(held, name),
          offer: printed(name).action
            ? null
            : {
                key: name,
                name,
                mode: world.neverOffer.includes(name) ? ("never" as const) : ("always" as const),
                ownTurn: false,
              },
        })),
      faked: world.fakedCards.filter(canFake).map(printed),
      reference: groups.map((group) => ({
        group,
        cards: names
          .filter((name) => printed(name).group === group)
          .map((name) => ({
            ...printed(name),
            total: copies(name),
            discarded: count(DISCARDED_CARDS, name),
            held: count(held, name),
            open: Math.max(0, copies(name) - count(DISCARDED_CARDS, name) - count(held, name)),
            faked: canFake(name) && world.fakedCards.includes(name),
          })),
      })),
    },
    secrets: {
      held: mine.filter((item) => item.state === "held").length,
      scored: mine.filter((item) => item.state === "scored").length,
      limit: 3,
      mine: mine
        .map((item) => ({ ...item, scored: item.state === "scored" }))
        .sort((a, b) => Number(a.scored) - Number(b.scored)),
      // The demo has a short deck: every objective of it is listed.
      total: secrets.length,
      unseen: secrets.filter((item) => item.state === "unseen").length,
      deck: 4,
      reference: SECRET_GROUPS.map((group) => {
        const cards = secrets.filter((item) => item.group === group);
        return { group, unseen: cards.filter((item) => item.state === "unseen").length, cards };
      }),
    },
    agendas: {
      laws: AGENDAS.flatMap(({ name, kind, text, against, passed }) =>
        passed
          ? [
              {
                name,
                kind,
                text: passed === "Against" ? (against ?? "") : text,
                outcome: seats.includes(passed) ? { seat: passed } : { text: passed },
              },
            ]
          : [],
      ),
      // The demo has a short deck: every agenda of it is listed.
      rest: AGENDAS.filter((item) => !item.passed).length,
      deck: 4,
      reference: AGENDA_GROUPS.map((group) => ({
        group,
        cards: AGENDAS.filter((item) => item.group === group && !item.passed).map(
          ({ name, kind, text, against }) => ({
            name,
            kind,
            text: [text, against].filter(Boolean).join("\n"),
          }),
        ),
      })).filter((group) => group.cards.length > 0),
    },
    exploration: EXPLORATION.map((deck) => ({
      ...deck,
      fragments: seats.flatMap((seat) =>
        (FRAGMENTS[seat] ?? [])
          .filter((item) => item.deck === deck.deck)
          .map((item) => ({ seat, count: item.count })),
      ),
    })),
    log: [3, 2].map((round) => ({
      round,
      current: round === 3,
      label: `Round ${round}${round === 3 ? " · Action phase" : " · complete"}`,
      entries: [
        ...(round === 3 ? current : []),
        ...HISTORY.filter((item) => item.round === round && seats.includes(item.seat)).map(entry),
      ],
    })),
  };
}

export function selectShell(world: World): ShellView {
  E.env.viewer = world.viewer;
  E.env.neverOffer = world.neverOffer;
  const state = currentState(world);
  const tactical = isTactical(state);
  const canApply =
    world.mode === "draft" &&
    tactical &&
    state.mode === "draft" &&
    E.canApply(state) &&
    !E.pendingEdit(state);
  return {
    seats: seatViews,
    accent:
      state.mode === "draft" ||
      E.flowStaged(state) ||
      E.decisionStaged(state) ||
      E.reactionPick(state)
        ? "draft"
        : state.frontier === null
          ? "done"
          : "live",
    toolbar: toolbar(world, state),
    players: players(world, state),
    board: selectBoard(world, state),
    action: tactical ? selectTactical(world, state) : selectFlow(world, state),
    reference: reference(world, state),
    apply: canApply
      ? {
          items: applyItems(state),
          cost: "Costs 1 tactic token.",
          note:
            state.frontier === null
              ? "The whole action is prepared."
              : "Dice and later decisions continue in Live.",
        }
      : null,
    reveal: state.reveal ?? null,
    toast: world.toast,
    announcement: world.announcement,
    // The demo always has a choice open, except for a past action.
    asked: world.mode !== "history",
  };
}
