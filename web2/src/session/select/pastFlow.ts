// A strategic or a component turn of the Log, opened: its action as Live showed it, read-only.
// A strategic action is its card and one row for each seat (the primary, then the secondaries),
// each with what it did in short and, one click further, line by line. A component action is the
// card or the ability that it used, as printed, with what it did as the chosen rows. Everything is
// of the public events of the game: the facts of the decisions and the state, and the board of
// the end of the turn taken back by the changes of the board (`history.ts`).
import type { BlockView, FlowActionView, MenuRowView, SeatRowView } from "../../model";
import type { EventLog } from "../events";
import { type PublicBoard, boardAt, withBoard } from "../history";
import type { Catalog, SecondaryFacts, SeatPools, SessionUpdate } from "../wire";
import { type SeatPart, turnEvents, turnParts } from "./log";
import { seatLabel, strategyCard } from "./names";
import { planetNames } from "./payment";
import { printedComponent, strategyCardHelp } from "./printed";

/** A strategic or a component turn of the Log that is open, and the game at its end. */
export interface PastFlow {
  /** The game with the board of the end of the turn. */
  update: SessionUpdate;
  /** The system that the action was about (the system of Diplomacy), marked on the board. */
  system: string | null;
  action: FlowActionView;
}

const NOTHING: FlowActionView["footer"] = { note: "", error: false, actions: [] };

/**
 * Of the turn `turn` of the Log: its strategic or component action, read-only, and the game with
 * the board of its end. Null for a turn of another action (a tactical action, a pass), or one that
 * the events no longer have.
 */
export function selectPastFlow(
  update: SessionUpdate,
  events: EventLog,
  turn: string,
  printed: Catalog | null = null,
): PastFlow | null {
  const parts = turnParts(update, events, turn, printed);
  const action = parts?.action;
  if (!parts || (action?.type !== "strategic" && action?.type !== "component")) {
    return null;
  }
  const { seat } = parts.view;
  const viewer = update.viewer.role === "player" ? update.viewer.seat : null;
  const name = (id: string) => (id === viewer ? "You" : seatLabel(update, id, printed));
  // The board of the end of the turn: after its last event, the change of the board of its step.
  const own = (turnEvents(events, turn)?.at ?? []).map((at) => ({ at, entry: events[at] }));
  const end = (own.at(-1)?.at ?? events.length - 1) + 1;
  const then = {
    ...update,
    view: withBoard(update.view, boardAt(update.view, events, end)),
    pending_choice: undefined,
  };
  // What the board says of the turn and no fact tells. A strategic action tells its command
  // tokens; a component action may change any pool.
  const before = boardAt(update.view, events, own[0]?.at ?? end);
  const changed = boardEffects(
    update,
    before,
    boardAt(update.view, events, end),
    action.type === "strategic" ? ["trade_goods", "commodities"] : POOLS.map(([pool]) => pool),
  );
  /** What a seat did: its facts, then what the board says that they do not. */
  const linesOf = (seat: string) => {
    const told = parts.seats.find((part) => part.seat === seat)?.lines ?? [];
    const more = (changed.get(seat) ?? []).filter(
      (line) => !told.some((fact) => line.startsWith("Readied ") && fact.includes(line.slice(8))),
    );
    return [...told, ...more];
  };
  const ended = own.some(
    ({ entry }) => entry.fact?.kind === "turn_ended" || entry.event.kind === "game_finished",
  );
  const pill = (live: boolean) =>
    live
      ? { tone: "live" as const, label: "Played now" }
      : { tone: "done" as const, label: "Resolved" };
  const frame = {
    badge: null,
    tabs: [],
    selected: 0,
    current: null,
    interrupt: null,
    closing: [],
    trail: [],
    footer: NOTHING,
  };

  if (action.type === "strategic") {
    const card = action.card ?? "";
    const printedCard = strategyCard(card, printed);
    // The strategic action that is played now: the game says who is still asked.
    const now =
      !ended && update.secondary?.primary === seat && update.secondary.card === card
        ? update.secondary
        : null;
    // The seats in their order from the primary, as the game asks them.
    const ids = update.view.players.map((player) => player.id);
    const at = ids.indexOf(seat);
    const order = [...ids.slice(at + 1), ...ids.slice(0, Math.max(at, 0))];
    const rows = strategicRows(
      order.map(
        (id) =>
          parts.seats.find((part) => part.seat === id) ?? {
            seat: id,
            followed: null,
            lines: [],
            summary: "",
          },
      ),
      parts.seats.find((part) => part.seat === seat),
      seat,
      viewer,
      name,
      now,
      linesOf,
    );
    const diplomacy = own.find(({ entry }) => entry.fact?.kind === "diplomacy_chosen")?.entry.fact;
    return {
      update: then,
      system: diplomacy?.kind === "diplomacy_chosen" ? diplomacy.system : null,
      action: {
        ...frame,
        kind: "flow",
        context: "Strategic action",
        contentKey: `past:${turn}`,
        heading: now?.name ?? printedCard.name,
        pill: pill(!!now),
        help: strategyCardHelp(card, printed),
        blocks: [{ kind: "seats", rows }],
      },
    };
  }

  // A component action: the card or the ability, as printed, and what it did as chosen rows:
  // the facts of its seats, then what the board says of it (a pool, a planet readied).
  const source = printedComponent(action.source, parts.card, printed);
  const lines = linesOf;
  const seats = [
    ...new Set([seat, ...parts.seats.map((part) => part.seat), ...changed.keys()]),
  ].filter((id) => lines(id).length);
  const chosen = (rows: string[]): MenuRowView[] =>
    rows.map((title) => ({
      key: "",
      title,
      selected: true,
      disabled: true,
      // A row of the Log is no control: the read-only scope shows it, and nothing sends it.
      intent: { type: "chooseOption", option: title },
    }));
  const blocks: BlockView[] = [
    ...(source ? [{ kind: "source" as const, ...source }] : []),
    ...seats.map((id) => ({
      kind: "menu" as const,
      ...(id === seat ? {} : { title: name(id) }),
      rows: chosen(lines(id)),
    })),
  ];
  const label = parts.view.label.replace(/^Component · /, "");
  const system = own
    .map(({ entry }) => entry.fact)
    .flatMap((fact) =>
      fact && "system" in fact && typeof fact.system === "string" ? [fact.system] : [],
    )[0];
  return {
    update: then,
    system: system ?? null,
    action: {
      ...frame,
      kind: "flow",
      context: "Component action",
      contentKey: `past:${turn}`,
      heading: source?.name ?? label,
      pill: pill(!ended),
      help: [],
      blocks: blocks.length
        ? blocks
        : [{ kind: "note", tone: "quiet", text: "The game does not tell what it did." }],
    },
  };
}

/**
 * The seats of a resolved strategic action, as the live action has them (`selectStrategy`): the
 * primary, then the other seats in the order that the game asks them, each with its answer and
 * what it did in short, and line by line one click further. While the action is played now, the
 * game says who is still asked (`now.followers`).
 */
function strategicRows(
  others: SeatPart[],
  own: SeatPart | undefined,
  primary: string,
  viewer: string | null,
  name: (id: string) => string,
  now: SecondaryFacts | null,
  linesOf: (seat: string) => string[],
): SeatRowView[] {
  const row = (seat: string, order: string, status: SeatRowView["status"], did?: SeatPart) => {
    const lines = linesOf(seat);
    // With no outcome to sum up, what the board says is the summary ("Trade goods 1 → 4").
    const text = did?.summary || lines.slice(0, 2).join(" · ");
    // The row opens only to what its summary does not say already.
    const more = lines.join(" · ").toLowerCase() !== text.toLowerCase();
    return {
      order,
      seat,
      name: name(seat),
      you: seat === viewer,
      status,
      ...(text ? { text } : {}),
      ...(more && lines.length ? { details: lines } : {}),
    };
  };
  const followers = now?.followers;
  // The seats that the game asks now: the followers of the live action.
  const seats = followers
    ? followers.flatMap((follower) => others.filter((part) => part.seat === follower.seat))
    : others;
  const deciding = now?.open
    ? followers?.find((follower) => follower.state === "waiting")?.seat
    : undefined;
  return [
    row(
      primary,
      "P",
      now && !now.open
        ? { tone: "live", sign: "▶", label: "Deciding" }
        : { tone: "done", sign: "✓", label: "Resolved" },
      own,
    ),
    ...seats.map((did, index) => {
      const state = followers?.find((follower) => follower.seat === did.seat)?.state;
      const followed = did.followed ?? (state === "followed" ? true : null);
      return row(
        did.seat,
        `${index + 1}`,
        followed
          ? { tone: "done", sign: "✓", label: "Followed" }
          : deciding === did.seat
            ? { tone: "live", sign: "▶", label: "Deciding" }
            : now && state === "waiting"
              ? { tone: "quiet", sign: "·", label: "Waits" }
              : { tone: "quiet", sign: "–", label: "Passed" },
        did,
      );
    }),
  ];
}

const POOLS: [keyof SeatPools, string][] = [
  ["trade_goods", "Trade goods"],
  ["commodities", "Commodities"],
  ["tactic_tokens", "Tactic pool"],
  ["fleet_tokens", "Fleet pool"],
  ["strategic_tokens", "Strategy pool"],
];

/**
 * What the board says of an action that no fact tells, by seat, in game values: a pool that
 * changed ("Trade goods 2 → 5"), a planet that was readied.
 */
export function boardEffects(
  update: SessionUpdate,
  before: PublicBoard,
  after: PublicBoard,
  pools: (keyof SeatPools)[],
): Map<string, string[]> {
  const out = new Map<string, string[]>();
  const add = (seat: string, line: string) => out.set(seat, [...(out.get(seat) ?? []), line]);
  for (const [seat, now] of Object.entries(after.pools)) {
    const was = before.pools[seat];
    for (const [pool, label] of POOLS) {
      if (was && pools.includes(pool) && was[pool] !== now[pool]) {
        add(seat, `${label} ${was[pool]} → ${now[pool]}`);
      }
    }
  }
  const names = planetNames(update);
  for (const [id, system] of Object.entries(after.board.systems)) {
    for (const [planet, now] of Object.entries(system.planets)) {
      const was = before.board.systems[id]?.planets[planet];
      if (was?.exhausted && !now.exhausted && now.controlled_by) {
        add(now.controlled_by, `Readied ${names.get(planet) ?? planet}`);
      }
    }
  }
  return out;
}
