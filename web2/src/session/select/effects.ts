// What the engine says of the effects of cards, in the names of the page: what playing a reaction
// card would do now (`previews` of the context of a question, the engine's `EffectPreview`), and
// which lingering effects of played cards are in force (`active_effects` of the view). The engine
// words each kind; this file only names the subjects (`phraseWords`). No rule is decided here.
import type { EffectView } from "../../model";
import type { Catalog, Choice, EffectPreview, SessionUpdate } from "../wire";
import { itemOf, phraseWords, type Worded } from "./notes";
import { sentence } from "./names";

/** A text of the engine as a sentence. */
const said = (words: Worded): Worded => ({ text: sentence(words.text), help: words.help });

/** The printed name of an action card by the alias that a preview names it by. */
const cardName = (card: string, update: SessionUpdate, catalog: Catalog | null) =>
  itemOf({ kind: "action_card", card }, update, catalog).text;

/**
 * What choosing `option` would do now, as the engine words it. A slot of several action cards is
 * one option with a preview for each card: `name` is the printed name of the card of the row.
 * Null: the engine says nothing of it.
 */
export function previewOf(
  choice: Choice | null | undefined,
  option: string,
  update: SessionUpdate,
  catalog: Catalog | null,
  name?: string,
): Worded | null {
  const previews = (choice?.context?.previews ?? []).filter(
    (item) => item.option === option && item.phrase?.text,
  );
  const preview: EffectPreview | undefined =
    previews.length > 1 && name
      ? previews.find((item) => cardName(item.card, update, catalog) === name)
      : previews[0];
  return preview ? said(phraseWords(preview.phrase, update, catalog)) : null;
}

/**
 * The preview of the copy `card` of an action card, when the engine has one. By the copy, not by
 * the name: another copy of the same card is another card, of another seat.
 */
export function previewOfCard(
  choice: Choice | null | undefined,
  card: string,
  update: SessionUpdate,
  catalog: Catalog | null,
): Worded | null {
  const preview = (choice?.context?.previews ?? []).find(
    (item) => item.card === card && item.phrase?.text,
  );
  return preview ? said(phraseWords(preview.phrase, update, catalog)) : null;
}

/** The lingering effects of played cards that are in force, as the toolbar shows them. */
export function activeEffects(update: SessionUpdate, catalog: Catalog | null): EffectView[] {
  return (update.view.active_effects ?? []).flatMap((effect) => {
    if (!effect.title?.text || !effect.summary?.text) {
      return [];
    }
    const title = phraseWords(effect.title, update, catalog);
    const summary = said(phraseWords(effect.summary, update, catalog));
    const help = [...title.help, ...summary.help].filter(
      (item, index, all) => all.findIndex((other) => other.title === item.title) === index,
    );
    return [{ title: sentence(title.text), summary: summary.text, expiry: effect.expiry, help }];
  });
}
