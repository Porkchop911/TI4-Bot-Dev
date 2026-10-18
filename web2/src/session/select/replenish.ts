// The replenish of commodities (`details.commodities`): the choice of other seats that replenish
// (Trade's primary: options of kind `replenish` with `details.seats`, and a decline), or the window
// of a secondary that replenishes the follower's own (`yes` and `no`). The game asks one seat at a
// time, and every question after the first offers the seats that are left; here it is one screen:
// the seats are chosen in the player table, and the main button answers every question at once,
// by the ids of their options.
import type { BlockView, PlayerPickView, RowView } from "../../model";
import { plural } from "../../model";
import type {
  Catalog,
  Choice,
  CommodityStanding,
  SessionUpdate,
  TradeOwnGain,
  TradeReplenishHints,
} from "../wire";
import { chooseFollow, followWindow, isFollowWindow } from "./follow";
import { seatLabel } from "./names";
import { type Editor, answered, changeControl, hintFieldsOf, hintOf } from "./editor";

/** What the player staged: the seats that replenish (by option id), or the answer of a window. */
export interface ReplenishDraft {
  seats: string[];
  follow: boolean | null;
}

export const NO_REPLENISH: ReplenishDraft = { seats: [], follow: null };

interface Offered {
  option: string;
  seat: string;
  have: number;
  max: number;
}

interface Question {
  /** The other seats are chosen, or the window of a secondary. */
  mode: "choose" | "follow";
  /** The seats that can be chosen, in the order of the options. */
  offered: Offered[];
  /** The option that chooses nobody more. */
  decline: string | null;
  /** What the card already gave the seat of the question (`details.own`). */
  own: TradeOwnGain | null;
  /** The follower's commodities now and at its limit. */
  standing: CommodityStanding | null;
}

function questionOf(choice: Choice): Question | null {
  if (isFollowWindow(choice)) {
    const standing = hintOf<CommodityStanding>(choice, "commodities");
    return standing ? { mode: "follow", offered: [], decline: null, own: null, standing } : null;
  }
  const told = hintFieldsOf<TradeReplenishHints>(choice, "commodities", "own");
  if (!told) {
    return null;
  }
  const seats = (choice.details?.seats ?? {}) as Record<string, unknown>;
  const choosing = choice.options.filter((option) => option.kind !== "decline");
  const offered = choosing.flatMap((option): Offered[] => {
    const seat = seats[option.id];
    const standing = told.commodities[option.id];
    return typeof seat === "string" && standing
      ? [{ option: option.id, seat, have: standing.have, max: standing.max }]
      : [];
  });
  if (!offered.length || offered.length !== choosing.length) {
    return null;
  }
  return {
    mode: "choose",
    offered,
    decline: choice.options.find((option) => option.kind === "decline")?.id ?? null,
    own: told.own,
    standing: null,
  };
}

export interface ReplenishView {
  blocks: BlockView[];
  /** The player table is the choice. Null for a window. */
  pick: PlayerPickView | null;
  note: string;
  /** Why nothing can be sent yet. Null: the main button sends. */
  problem: string | null;
  label: string;
  /** Something is staged: Esc clears it. */
  changed: boolean;
  /** The answers, by the ids of their options, in the order that the game asks. Null: not yet. */
  answers: string[] | null;
}

/** "A, B and C" */
const names = (list: string[]) =>
  list.length < 2 ? (list[0] ?? "") : `${list.slice(0, -1).join(", ")} and ${list.at(-1)}`;

/** The staged seats that the question still offers, in the order of the options. */
const stagedOf = (question: Question, draft: ReplenishDraft | null) =>
  question.offered.filter((item) => (draft ?? NO_REPLENISH).seats.includes(item.option));

/** The replenish of commodities, of other seats or of the follower. Null for any other question. */
export function selectReplenish(
  update: SessionUpdate,
  choice: Choice,
  draft: ReplenishDraft | null,
  printed: Catalog | null = null,
): ReplenishView | null {
  const question = questionOf(choice);
  if (!question) {
    return null;
  }
  const commodities = (have: number, max: number) => `Commodities ${have} → ${max}`;
  if (question.mode === "follow") {
    const { have, max } = question.standing!;
    return {
      ...followWindow(
        choice,
        draft?.follow ?? null,
        "Replenish commodities",
        commodities(have, max),
        "replenish commodities",
      ),
      pick: null,
    };
  }

  const chosen = stagedOf(question, draft);
  const name = (seat: string) => seatLabel(update, seat, printed);
  const blocks: BlockView[] = [];
  const own = question.own;
  if (own) {
    const gained = own.trade_goods.after - own.trade_goods.before;
    blocks.push({
      kind: "card",
      title: "Your seat",
      rows: [
        {
          icon: "card",
          title: [`Gain ${plural(gained, "trade good")}`],
          subtitle: `Trade goods ${own.trade_goods.before} → ${own.trade_goods.after}`,
        },
        {
          icon: "card",
          title: ["Replenish commodities"],
          subtitle: commodities(own.commodities.before, own.commodities.after),
        },
      ],
    });
  }
  blocks.push({
    kind: "card",
    title: "Other players",
    aside: `${chosen.length} / ${question.offered.length} chosen · Choose in the player table`,
    rows: chosen.length
      ? chosen.map((item): RowView => ({
          icon: "card",
          title: [{ seat: item.seat }, ` ${name(item.seat)}`],
          subtitle: `${commodities(item.have, item.max)} · no strategy token`,
          control: changeControl(`seat:${item.option}`),
        }))
      : [{ icon: "card", title: ["No player chosen"] }],
  });
  const pick: PlayerPickView = {
    verb: "Choose",
    seats: Object.fromEntries(
      question.offered.map((item) => [
        item.seat,
        { reason: null, note: commodities(item.have, item.max) },
      ]),
    ),
    chosen: chosen.map((item) => item.seat),
  };
  // Every seat that is not chosen is declined at once; when all are chosen the game asks no more.
  const answers = [
    ...chosen.map((item) => item.option),
    ...(chosen.length < question.offered.length && question.decline ? [question.decline] : []),
  ];
  const who = names(chosen.map((item) => name(item.seat)));
  return {
    blocks,
    pick,
    note: chosen.length
      ? `${who} ${chosen.length === 1 ? "replenishes" : "replenish"} commodities`
      : "Choose players in the player table, or nobody.",
    problem: null,
    label: chosen.length ? "Resolve primary" : "Resolve primary · nobody else",
    changed: chosen.length > 0,
    answers,
  };
}

/** A seat of the player table is chosen, or given back. */
export function toggleReplenishSeat(
  choice: Choice,
  draft: ReplenishDraft | null,
  seat: string,
): ReplenishDraft | null {
  const question = questionOf(choice);
  const option = question?.offered.find((item) => item.seat === seat)?.option;
  if (question?.mode !== "choose" || !option) {
    return null;
  }
  const staging = draft ?? NO_REPLENISH;
  return staging.seats.includes(option)
    ? { ...staging, seats: staging.seats.filter((id) => id !== option) }
    : { ...staging, seats: [...staging.seats, option] };
}

/** The answer of a window: `yes` follows, `no` passes. The same answer again takes it back. */
export function chooseReplenish(
  choice: Choice,
  draft: ReplenishDraft | null,
  group: string | undefined,
  option: string,
): ReplenishDraft | null {
  const staging = draft ?? NO_REPLENISH;
  const chosen =
    questionOf(choice)?.mode === "follow" ? chooseFollow(staging.follow, group, option) : null;
  return chosen ? { ...staging, ...chosen } : null;
}

/** "Change" of a chosen seat (`seat:<option>`): it is given back. */
export function clearReplenishPart(
  choice: Choice,
  draft: ReplenishDraft | null,
  part: string,
): ReplenishDraft | null {
  const option = part.startsWith("seat:") ? part.slice(5) : null;
  if (!questionOf(choice) || !draft || !option || !draft.seats.includes(option)) {
    return null;
  }
  return { ...draft, seats: draft.seats.filter((id) => id !== option) };
}

/** Esc clears the last choice: the last seat, or the answer of the window. */
export function clearLastReplenish(
  choice: Choice,
  draft: ReplenishDraft | null,
): ReplenishDraft | null {
  const question = questionOf(choice);
  if (!question || !draft) {
    return null;
  }
  if (question.mode === "follow") {
    return draft.follow !== null ? NO_REPLENISH : null;
  }
  const chosen = draft.seats.filter((id) => question.offered.some((item) => item.option === id));
  return chosen.length ? { ...draft, seats: chosen.slice(0, -1) } : null;
}

/** The replenish of commodities as an editor (`editors.ts`). */
export const replenishEditor: Editor<ReplenishDraft> = {
  view: (update, choice, draft, printed) =>
    answered(selectReplenish(update, choice, draft, printed)),
  seat: toggleReplenishSeat,
  option: chooseReplenish,
  part: clearReplenishPart,
  last: clearLastReplenish,
};
