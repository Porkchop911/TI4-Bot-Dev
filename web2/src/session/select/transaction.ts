// A transaction on one screen: what the viewer gives and what the viewer gets, side by side, from
// what the engine says each offer moves (`details.transaction`, the engine's `TransactionHints`).
// The proposer stages both sides with counters and cards; the staged deal is an offer of the
// engine, or it cannot be sent, and the offers that hold what is staged are listed under it, one
// tap each. The partner sees the deal on the table the same way, read-only, with the answers of
// the engine. Presentation only: every offer comes from the engine, none is made here.
import type {
  BlockView,
  CounterView,
  DealItemView,
  DealSideView,
  HelpView,
  MenuRowView,
} from "../../model";
import { plural } from "../../model";
import type { Catalog, Choice, SessionUpdate, TradeDeal, TradeItem } from "../wire";
import { type Editor, answered } from "./editor";
import { humanizeId, seatLabel, sentence } from "./names";
import { itemOf, transactionOf, worded } from "./notes";

/** A staged side: the amount of each thing, by its key (`keyOf`). A card is 1. */
type Amounts = Record<string, number>;

/** What the player staged: the two sides of the proposer, or the answer of the partner. */
export interface TransactionDraft {
  give: Amounts;
  get: Amounts;
  answer: string | null;
}

const EMPTY: TransactionDraft = { give: {}, get: {}, answer: null };

/** The sides of a deal, from the chair of the viewer. */
type Side = "give" | "get";

/** The kinds of things in the order that a side lists them. */
const ORDER: TradeItem["kind"][] = [
  "trade_goods",
  "commodities",
  "relic_fragments",
  "note",
  "action_card",
  "secret_objective",
];

const GROUP: Partial<Record<TradeItem["kind"], string>> = {
  note: "Promissory notes",
  action_card: "Action cards",
  secret_objective: "Secret objectives",
};

/** The identity of a thing that a side hands over, whatever its amount. */
function keyOf(item: TradeItem): string {
  switch (item.kind) {
    case "trade_goods":
    case "commodities":
      return item.kind;
    case "relic_fragments":
      return `relic_fragments:${item.trait}`;
    case "note":
      return `note:${item.card}:${item.owner}`;
    case "action_card":
    case "secret_objective":
      return `${item.kind}:${item.card}`;
  }
}

/** The amount of a thing: a count, or 1 for a card. */
const amountOf = (item: TradeItem) => ("count" in item ? item.count : 1);

const counted = (item: TradeItem) => "count" in item;

function amounts(items: TradeItem[]): Amounts {
  const out: Amounts = {};
  for (const item of items) {
    out[keyOf(item)] = (out[keyOf(item)] ?? 0) + amountOf(item);
  }
  return out;
}

/** The staged amounts without the zeros. */
const nonZero = (side: Amounts) => Object.entries(side).filter(([, value]) => value > 0);

const same = (a: Amounts, b: Amounts) => {
  const left = nonZero(a);
  return left.length === nonZero(b).length && left.every(([key, value]) => b[key] === value);
};

/** Every amount of `inner` is in `outer`. */
const holds = (outer: Amounts, inner: Amounts) =>
  nonZero(inner).every(([key, value]) => (outer[key] ?? 0) >= value);

/** How far apart two sides are: the sum of the differences of their amounts. */
function distance(a: Amounts, b: Amounts) {
  const keys = new Set([...Object.keys(a), ...Object.keys(b)]);
  return [...keys].reduce((sum, key) => sum + Math.abs((a[key] ?? 0) - (b[key] ?? 0)), 0);
}

const isEmpty = (draft: TransactionDraft) =>
  !nonZero(draft.give).length && !nonZero(draft.get).length;

/** An offer of the proposer, from the chair of the viewer. */
interface Offer {
  id: string;
  deal: TradeDeal;
  give: Amounts;
  get: Amounts;
}

/** What a side can hand over in any offer: each thing once, with the most that an offer has. */
interface Thing {
  key: string;
  item: TradeItem;
  max: number;
}

function things(lists: TradeItem[][]): Thing[] {
  const found = new Map<string, Thing>();
  for (const item of lists.flat()) {
    const key = keyOf(item);
    const max = Math.max(found.get(key)?.max ?? 0, amountOf(item));
    found.set(key, { key, item, max });
  }
  return [...found.values()].sort(
    (a, b) => ORDER.indexOf(a.item.kind) - ORDER.indexOf(b.item.kind),
  );
}

/** The question of the proposer, or of the partner. Null for any other question. */
function questionOf(choice: Choice) {
  const hints = transactionOf(choice);
  if (!hints) {
    return null;
  }
  if (hints.offer) {
    return { hints, offer: hints.offer, offers: [] as Offer[] };
  }
  const offers = choice.options.flatMap((option): Offer[] => {
    const deal = hints.offers[option.id];
    return deal
      ? [{ id: option.id, deal, give: amounts(deal.given), get: amounts(deal.received) }]
      : [];
  });
  return offers.length ? { hints, offer: null, offers } : null;
}

/** "Trade goods 3 · Commodities 2": what a seat holds now. */
function holdingsOf(update: SessionUpdate, seat: string) {
  const player = update.view.players.find((item) => item.id === seat);
  return player ? `Trade goods ${player.trade_goods} · Commodities ${player.commodities}` : "";
}

/** The name of a thing without its amount, and the printed text of a card. */
function nameOf(
  item: TradeItem,
  update: SessionUpdate,
  printed: Catalog | null,
): { name: string; help?: HelpView } {
  switch (item.kind) {
    case "trade_goods":
      return { name: "Trade goods" };
    case "commodities":
      return { name: "Commodities" };
    case "relic_fragments":
      return { name: `${humanizeId(item.trait)} relic fragments` };
    default: {
      const named = itemOf(item, update, printed);
      return { name: named.text, ...(named.help?.text ? { help: named.help } : {}) };
    }
  }
}

/** The things of a side, staged with counters and cards. */
function stagedItems(
  side: Side,
  list: Thing[],
  staged: Amounts,
  update: SessionUpdate,
  printed: Catalog | null,
): DealItemView[] {
  return list.map((thing) => {
    const named = nameOf(thing.item, update, printed);
    const group = GROUP[thing.item.kind];
    const value = staged[thing.key] ?? 0;
    if (counted(thing.item)) {
      const counter: CounterView = {
        id: `${side}|${thing.key}`,
        value,
        // A staged amount stays in reach, even over what the offers have.
        max: Math.max(thing.max, value),
        label: named.name.toLowerCase(),
      };
      return { ...named, counter };
    }
    return {
      ...named,
      ...(group ? { group } : {}),
      toggle: {
        on: value > 0,
        intent: { type: "chooseOption", group: side, option: thing.key },
      },
    };
  });
}

/** The things of a side of a deal that is read-only. */
function readItems(
  items: TradeItem[],
  update: SessionUpdate,
  printed: Catalog | null,
): DealItemView[] {
  return things([items]).map((thing) => {
    const named = nameOf(thing.item, update, printed);
    const group = GROUP[thing.item.kind];
    return counted(thing.item)
      ? { ...named, count: thing.max }
      : { ...named, ...(group ? { group } : {}) };
  });
}

/** How many offers the list shows at most: the composer narrows the rest. */
const SHOWN = 6;
const KEYS = "123456789";

export interface TransactionView {
  blocks: BlockView[];
  note: string;
  problem: string | null;
  label: string;
  changed: boolean;
  answers: string[] | null;
}

/** The screen of a transaction. Null for any other question. */
export function selectTransaction(
  update: SessionUpdate,
  choice: Choice,
  draft: TransactionDraft | null,
  printed: Catalog | null = null,
): TransactionView | null {
  const question = questionOf(choice);
  if (!question) {
    return null;
  }
  const staged = draft ?? EMPTY;
  const { hints } = question;
  const side = (title: string, seat: string, items: DealItemView[]): DealSideView => ({
    title,
    seat,
    name: seatLabel(update, seat, printed),
    holds: holdingsOf(update, seat),
    items,
  });

  // The partner answers the deal on the table: the proposer gives what the viewer gets.
  if (question.offer) {
    const offer = question.offer;
    const answers = choice.options;
    const chosen = answers.find((option) => option.id === staged.answer) ?? null;
    const rows: MenuRowView[] = answers.map((option, index) => ({
      key: KEYS[index] ?? "",
      title: sentence(option.label || option.id),
      selected: option.id === chosen?.id,
      disabled: false,
      intent: { type: "chooseOption", group: "answer", option: option.id },
    }));
    const proposer = seatLabel(update, hints.proposer, printed);
    return {
      blocks: [
        {
          kind: "deal",
          sides: [
            side("You give", hints.partner, readItems(offer.received, update, printed)),
            side("You get", hints.proposer, readItems(offer.given, update, printed)),
          ],
        },
        { kind: "menu", title: "Answer", rows },
      ],
      note: chosen
        ? `Selected: ${sentence(chosen.label || chosen.id)} · ${worded(`${proposer} gives`, offer, update, printed).text}`
        : `Accept, refuse or counter the offer of ${proposer}.`,
      problem: chosen ? null : "Choose an answer.",
      label: chosen ? sentence(chosen.label || chosen.id) : "Answer",
      changed: !!chosen,
      answers: chosen ? [chosen.id] : null,
    };
  }

  // The proposer stages both sides; the staged deal is one of the offers of the engine.
  const offers = question.offers;
  const matched = offers.find(
    (offer) => same(offer.give, staged.give) && same(offer.get, staged.get),
  );
  const empty = isEmpty(staged);
  const decline = choice.options.find((option) => option.kind === "decline") ?? null;
  // The offers that hold everything that is staged, the closest first; when none does, the
  // offers nearest to it.
  const closeness = (offer: Offer) =>
    distance(offer.give, staged.give) + distance(offer.get, staged.get);
  const holding = offers.filter(
    (offer) => holds(offer.give, staged.give) && holds(offer.get, staged.get),
  );
  const nearest = !holding.length;
  const listed = (nearest ? offers : holding)
    .map((offer, index) => ({ offer, index, far: closeness(offer) }))
    .sort((a, b) => (empty ? a.index - b.index : a.far - b.far || a.index - b.index))
    .slice(0, nearest ? 3 : SHOWN)
    .map((item) => item.offer);
  const rows: MenuRowView[] = listed.map((offer, index) => {
    const words = worded("Give", offer.deal, update, printed);
    return {
      key: KEYS[index] ?? "",
      title: words.text,
      ...(words.help.length ? { help: words.help } : {}),
      selected: offer.id === matched?.id,
      disabled: false,
      intent: { type: "chooseOption", group: "offer", option: offer.id },
    };
  });
  // The nearest offers are a few; the rest is narrowed by what is staged.
  const more = nearest ? 0 : holding.length - listed.length;
  const partner = seatLabel(update, hints.partner, printed);
  const words = matched ? worded("Give", matched.deal, update, printed) : null;
  const problem = empty || matched ? null : `No offer to ${partner} is this deal.`;
  return {
    blocks: [
      {
        kind: "deal",
        sides: [
          side(
            "You give",
            hints.proposer,
            stagedItems(
              "give",
              things(offers.map((offer) => offer.deal.given)),
              staged.give,
              update,
              printed,
            ),
          ),
          side(
            "You get",
            hints.partner,
            stagedItems(
              "get",
              things(offers.map((offer) => offer.deal.received)),
              staged.get,
              update,
              printed,
            ),
          ),
        ],
      },
      {
        kind: "menu",
        title: nearest
          ? "No offer has all of this · the nearest offers"
          : empty
            ? "Offers"
            : `Offers with this · ${holding.length} of ${offers.length}`,
        rows: more > 0 ? [...rows, moreRow(more)] : rows,
      },
    ],
    note: words
      ? `Selected: ${words.text}`
      : problem
        ? problem
        : `Stage what you give and what you get, or choose an offer.`,
    problem,
    label: empty && decline ? "Offer nothing" : "Propose",
    changed: !empty,
    answers: matched ? [matched.id] : empty && decline ? [decline.id] : null,
  };
}

/** The row that says how many offers more there are: what is staged narrows them. */
const moreRow = (more: number): MenuRowView => ({
  key: "",
  title: `${plural(more, "offer")} more`,
  state: "Stage a part of the deal to narrow them",
  disabled: true,
  intent: { type: "clearChoice" },
});

/** A counter of a side: `give|<key>` or `get|<key>`. */
function countTransaction(
  choice: Choice,
  draft: TransactionDraft | null,
  key: string,
  value: number,
): TransactionDraft | null {
  const [side, thing] = [key.slice(0, key.indexOf("|")), key.slice(key.indexOf("|") + 1)];
  if (!questionOf(choice) || (side !== "give" && side !== "get") || value < 0) {
    return null;
  }
  const staged = draft ?? EMPTY;
  return { ...staged, [side]: { ...staged[side], [thing]: value } };
}

/** A card of a side is staged or taken back; an offer is staged whole; an answer is chosen. */
function chooseTransaction(
  choice: Choice,
  draft: TransactionDraft | null,
  group: string | undefined,
  option: string,
): TransactionDraft | null {
  const question = questionOf(choice);
  if (!question) {
    return null;
  }
  const staged = draft ?? EMPTY;
  if (group === "answer") {
    return question.offer && choice.options.some((item) => item.id === option)
      ? { ...staged, answer: staged.answer === option ? null : option }
      : null;
  }
  if (group === "offer") {
    const offer = question.offers.find((item) => item.id === option);
    return offer ? { ...staged, give: { ...offer.give }, get: { ...offer.get } } : null;
  }
  if (group === "give" || group === "get") {
    const on = (staged[group][option] ?? 0) > 0;
    return { ...staged, [group]: { ...staged[group], [option]: on ? 0 : 1 } };
  }
  return null;
}

/** The transaction as an editor (`editors.ts`). Esc clears the whole screen. */
export const transactionEditor: Editor<TransactionDraft> = {
  view: (update, choice, draft, printed) =>
    answered(selectTransaction(update, choice, draft, printed)),
  count: countTransaction,
  option: chooseTransaction,
};
