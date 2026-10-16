// What a transaction moves, in the names of the page: the engine says it as typed data
// (`details.transaction`, the engine's `TransactionHints`), each card by its record in the
// catalogue, and this file words it. A text of the engine that names a note or a faction by its id
// outside a transaction (`namedText`) is the fallback. Presentation only: no rule is decided here.
import type { HelpView } from "../../model";
import { plural } from "../../model";
import type {
  Catalog,
  Choice,
  PromissoryNoteRecord,
  SessionUpdate,
  TradeDeal,
  TradeItem,
  TransactionHints,
} from "../wire";
import { plain } from "./components";
import { hintOf } from "./editor";
import { factionName, humanizeId, seatLabel, sentence } from "./names";

/** The faction of the viewer, when the viewer has a seat. */
function viewerFaction(update: SessionUpdate): string | null {
  const seat = update.viewer.role === "player" ? update.viewer.seat : null;
  return update.view.players.find((player) => player.id === seat)?.faction ?? null;
}

export interface PrintedNote {
  /** "Trade Agreement (Nomad)": the printed name, and the owner of a note that every faction has when it is not the viewer. */
  label: string;
  /** The printed text, the owner named where it prints "<color>". */
  text: string;
}

/** A note as printed, by its record of the catalogue and the faction whose copy it is. */
function printed(
  record: PromissoryNoteRecord,
  owner: string,
  update: SessionUpdate,
  catalog: Catalog | null,
): PrintedNote {
  const name = factionName(owner, catalog);
  // A note of a faction says whose it is; a note that every faction has does not.
  const generic = !record.faction;
  return {
    label: generic && owner !== viewerFaction(update) ? `${record.name} (${name})` : record.name,
    text: plain(record.text).replaceAll("<color>", name),
  };
}

const noteRecord = (card: string, catalog: Catalog | null) =>
  catalog?.records.promissory_notes.find((item) => item.id === card) ?? null;

/** A part of a deal, as the page names it, with its printed text for "?" when it is a card. */
interface Named {
  text: string;
  help: HelpView | null;
}

function card(cards: { id: string; name: string; text: string }[] | undefined, id: string): Named {
  const found = cards?.find((item) => item.id === id);
  return found
    ? { text: found.name, help: { title: found.name, text: plain(found.text) } }
    : { text: humanizeId(id), help: null };
}

function itemOf(item: TradeItem, update: SessionUpdate, catalog: Catalog | null): Named {
  switch (item.kind) {
    case "trade_goods":
      return { text: plural(item.count, "trade good"), help: null };
    case "commodities":
      return { text: plural(item.count, "commodity", "commodities"), help: null };
    case "relic_fragments":
      return {
        text: plural(item.count, `${humanizeId(item.trait)} relic fragment`),
        help: null,
      };
    case "note": {
      const record = noteRecord(item.card, catalog);
      if (!record) {
        return { text: humanizeId(item.card), help: null };
      }
      const note = printed(record, item.owner, update, catalog);
      return { text: note.label, help: { title: note.label, text: note.text } };
    }
    case "action_card":
      return card(catalog?.records.action_cards, item.card);
    case "secret_objective":
      return card(catalog?.records.secret_objectives, item.card);
  }
}

/** A text of the page, and the printed text of the cards that it names, for its "?". */
export interface Worded {
  text: string;
  help: HelpView[];
}

function side(items: TradeItem[], update: SessionUpdate, catalog: Catalog | null) {
  const named = items.map((item) => itemOf(item, update, catalog));
  return {
    text: named.map((item) => item.text).join(", ") || "nothing",
    help: named.flatMap((item) => (item.help?.text ? [item.help] : [])),
  };
}

function worded(
  lead: string,
  deal: TradeDeal,
  update: SessionUpdate,
  catalog: Catalog | null,
): Worded {
  const given = side(deal.given, update, catalog);
  const received = side(deal.received, update, catalog);
  const text = !deal.received.length
    ? `${lead} ${given.text}`
    : `${lead} ${given.text} for ${received.text}`;
  const help = [...given.help, ...received.help].filter(
    (item, index, all) => all.findIndex((other) => other.title === item.title) === index,
  );
  return { text, help };
}

/** The typed contents of the questions of a negotiation. Null for any other question. */
export const transactionOf = (choice: Choice | null | undefined): TransactionHints | null =>
  choice ? hintOf<TransactionHints>(choice, "transaction") : null;

/**
 * An offer of the proposer, by the id of its option: "Give Ceasefire for 2 trade goods". Null for
 * an option that is no deal (the decline).
 */
export function transactionOption(
  hints: TransactionHints,
  option: string,
  update: SessionUpdate,
  catalog: Catalog | null,
): Worded | null {
  const deal = hints.offers[option];
  return deal ? worded("Give", deal, update, catalog) : null;
}

/**
 * The question of a negotiation: "Transaction with Jol-Nar" for the proposer, and the deal on the
 * table for the partner ("Hacan gives Ceasefire (Hacan) for 2 trade goods").
 */
export function transactionPrompt(
  hints: TransactionHints,
  update: SessionUpdate,
  catalog: Catalog | null,
): Worded {
  if (hints.offer) {
    return worded(
      `${seatLabel(update, hints.proposer, catalog)} gives`,
      hints.offer,
      update,
      catalog,
    );
  }
  return { text: `Transaction with ${seatLabel(update, hints.partner, catalog)}`, help: [] };
}

// ---- The fallback: a text of the engine that names notes and factions by id --------------------

/** A note that every faction has is `<color>_<alias>` in the catalogue. */
const GENERIC = "<color>_";
/** Support for the Throne is `support:<owner>` in the engine and `<color>_sftt` in the catalogue. */
const SUPPORT = "support";
const SUPPORT_ALIAS = "sftt";

/** A note id of the engine ("cf:sol"), as printed. Null for an id that is no note of the catalogue. */
function printedNote(note: string, update: SessionUpdate, catalog: Catalog | null) {
  const [alias, owner] = note.split(":");
  if (!alias || !owner || note.slice(alias.length + 1).includes(":")) {
    return null;
  }
  const key = alias === SUPPORT ? SUPPORT_ALIAS : alias;
  const record = noteRecord(key, catalog) ?? noteRecord(GENERIC + key, catalog);
  return record ? printed(record, owner, update, catalog) : null;
}

const escapeRegExp = (text: string) => text.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/**
 * A text of the engine with its note ids and the faction ids of the table in the names of the
 * page, for the questions that do not say what they name as typed data (a note given with an
 * ability, "{faction} becomes speaker"): a note as printed, a faction as the page calls its seat
 * (`seatLabel`). `help` has the printed text of the notes that it names.
 */
export function namedText(text: string, update: SessionUpdate, catalog: Catalog | null): Worded {
  const seats = update.view.players.filter((player) => player.faction);
  const factions = seats.map((player) => escapeRegExp(player.faction));
  const pattern = new RegExp(
    `\\b[a-z0-9_]+:[a-z0-9_]+\\b${factions.length ? `|\\b(?:${factions.join("|")})\\b` : ""}`,
    "g",
  );
  const help: HelpView[] = [];
  const named = text.replace(pattern, (token) => {
    if (token.includes(":")) {
      const note = printedNote(token, update, catalog);
      if (note?.text && !help.some((item) => item.title === note.label)) {
        help.push({ title: note.label, text: note.text });
      }
      return note?.label ?? token;
    }
    const seat = seats.find((player) => player.faction === token);
    return seat ? seatLabel(update, seat.id, catalog) : token;
  });
  return { text: named, help };
}

/** The text of an option of a question, in the names of the page: its deal, else `namedText`. */
export function optionWords(
  choice: Choice | null | undefined,
  option: { id: string; label: string },
  update: SessionUpdate,
  catalog: Catalog | null,
): Worded {
  const hints = transactionOf(choice);
  const deal = hints ? transactionOption(hints, option.id, update, catalog) : null;
  if (deal) {
    return deal;
  }
  const named = namedText(option.label || option.id, update, catalog);
  return { text: sentence(named.text), help: named.help };
}

/** The question in the names of the page: its deal, else `namedText`. */
export function promptWords(
  choice: Choice | null | undefined,
  update: SessionUpdate,
  catalog: Catalog | null,
): Worded {
  const hints = transactionOf(choice);
  if (hints) {
    return transactionPrompt(hints, update, catalog);
  }
  const named = namedText(choice?.prompt ?? "", update, catalog);
  return { text: sentence(named.text), help: named.help };
}
