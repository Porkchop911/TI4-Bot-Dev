// What a transaction moves, and what a text of the engine names by id, in the names of the page:
// the engine says it as typed data (`details.transaction`, the engine's `TransactionHints`, and
// `details.phrases`, its `PhraseHints`), each card by its record in the catalogue, and this file
// words it. A text with no typed data is shown as the engine says it. Presentation only: no rule
// is decided here.
import type { HelpView } from "../../model";
import { plural } from "../../model";
import type {
  Catalog,
  Choice,
  Phrase,
  PhraseHints,
  PromissoryNoteRecord,
  SessionUpdate,
  Subject,
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
export interface Named {
  text: string;
  help: HelpView | null;
}

function card(
  cards: { id: string; name: string; text: string; automation_id?: string }[] | undefined,
  id: string,
): Named {
  // An action card of the engine is a copy of the catalogue ("mb1") or its name in the rules
  // ("bribery", `automation_id`).
  const found =
    cards?.find((item) => item.id === id) ?? cards?.find((item) => item.automation_id === id);
  return found
    ? { text: found.name, help: { title: found.name, text: plain(found.text) } }
    : { text: humanizeId(id), help: null };
}

export function itemOf(item: TradeItem, update: SessionUpdate, catalog: Catalog | null): Named {
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

/** A deal in words: "<lead> <given> for <received>", with the printed text of its cards. */
export function worded(
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
 * The question of a negotiation: "Transaction with Jol-Nar" for the proposer, and "Answer Hacan’s
 * offer" for the partner. The deal itself is the block of the screen (`transaction.ts`), with the
 * printed text of its cards.
 */
export function transactionPrompt(
  hints: TransactionHints,
  update: SessionUpdate,
  catalog: Catalog | null,
): Worded {
  return hints.offer
    ? { text: `Answer ${seatLabel(update, hints.proposer, catalog)}’s offer`, help: [] }
    : { text: `Transaction with ${seatLabel(update, hints.partner, catalog)}`, help: [] };
}

// ---- A text of the engine that names a seat, a note or a card by its id ---------------------

/** What the texts of a question name by id (`details.phrases`, the engine's `PhraseHints`). */
export const phrasesOf = (choice: Choice | null | undefined): PhraseHints | null =>
  choice ? hintOf<PhraseHints>(choice, "phrases") : null;

/** A thing that a text names, as the page names it: a seat by `seatLabel`, a card as printed. */
function subjectOf(subject: Subject, update: SessionUpdate, catalog: Catalog | null): Named {
  switch (subject.kind) {
    case "seat":
      return { text: seatLabel(update, subject.player, catalog), help: null };
    case "note":
    case "action_card":
    case "secret_objective":
      return itemOf(subject, update, catalog);
    // The subjects of the effect texts, which no question of this page names yet.
    case "seat_possessive":
      return { text: `${seatLabel(update, subject.player, catalog)}’s`, help: null };
    case "system":
      return { text: subject.system, help: null };
    case "planet":
      return { text: humanizeId(subject.planet), help: null };
    case "unit":
      return { text: humanizeId(subject.unit_type), help: null };
    case "units":
      return { text: `${subject.count} ${humanizeId(subject.unit_type)}`, help: null };
    case "agenda":
    case "strategy_card":
      return { text: humanizeId(subject.card), help: null };
  }
}

/**
 * A text of the engine in the names of the page: each `{key}` of the phrase is its subject, and
 * `help` has the printed text of the cards that it names.
 */
export function phraseWords(
  phrase: Phrase,
  update: SessionUpdate,
  catalog: Catalog | null,
): Worded {
  const help: HelpView[] = [];
  const text = phrase.text.replace(/\{(\w+)\}/g, (token, key: string) => {
    const subject = phrase.refs[key];
    if (!subject) {
      return token;
    }
    const named = subjectOf(subject, update, catalog);
    if (named.help?.text && !help.some((item) => item.title === named.help?.title)) {
      help.push(named.help);
    }
    return named.text;
  });
  return { text, help };
}

/**
 * The text of an option of a question, in the names of the page: its deal, else its phrase, else
 * the label of the engine.
 */
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
  const phrase = phrasesOf(choice)?.options?.[option.id];
  const named = phrase
    ? phraseWords(phrase, update, catalog)
    : { text: option.label || option.id, help: [] };
  return { text: sentence(named.text), help: named.help };
}

/** The question in the names of the page: its deal, else its phrase, else the prompt of the engine. */
export function promptWords(
  choice: Choice | null | undefined,
  update: SessionUpdate,
  catalog: Catalog | null,
): Worded {
  const hints = transactionOf(choice);
  if (hints) {
    return transactionPrompt(hints, update, catalog);
  }
  const phrase = phrasesOf(choice)?.prompt;
  const named = phrase
    ? phraseWords(phrase, update, catalog)
    : { text: choice?.prompt ?? "", help: [] };
  return { text: sentence(named.text), help: named.help };
}
