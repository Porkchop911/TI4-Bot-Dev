import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import {
  findActionCardByName,
  findActionCardMeta,
  findSecretObjectiveMeta,
  humanizeId,
} from "../protocol/contentCatalog.ts";
import { GENERATED_CONTENT_CATALOG } from "../protocol/generatedContentManifest.ts";

export interface CardOptionInfo {
  title: string;
  /** The card's printed text, when the catalog has it. */
  text: string | null;
  /** A short fact such as "Action phase" or "1 VP". */
  badge: string | null;
}

const DISCARD_SUBTYPES = new Set(["discard_over_hand_limit", "expedition_discard_action_card"]);
/** Cards another player has shown you (Mageon Implants, Spy Net): choose one to take. */
const TAKE_SUBTYPES = new Set(["take_revealed_action_card"]);
const SECRET_SUBTYPES = new Set(["return_over_secret_hand_limit", "expedition_discard_secret"]);

/** Diplomatic Pressure (and any card) asking a seat which promissory note to give: `<card>_pick_promissory note`. */
const NOTE_PICK_SUBTYPE = /_pick_promissory note$/;

type NoteRecord = { name: string; text?: string };
const NOTES = GENERATED_CONTENT_CATALOG.promissoryNotes as unknown as Record<string, NoteRecord>;

/** A note id is `alias:Owner faction` (Support for the Throne: `support:Owner`). */
function describeNote(id: string): CardOptionInfo {
  const support = id.startsWith("support:");
  const [alias, ownerPart] = support ? ["sftt", id.slice("support:".length)] : id.split(/:(.*)/s);
  const owner = ownerPart || null;
  const record = NOTES[`<color>_${alias}`] ?? NOTES[alias];
  if (!record) return { title: humanizeId(alias), text: null, badge: owner ? `${owner}'s note` : null };
  const who = owner ? `the ${owner} player` : "the issuing player";
  return {
    title: record.name,
    text: record.text ? record.text.replace(/the <color> player/g, who).replace(/<color>/g, owner ?? "issuing") : null,
    badge: owner ? `Promissory note · ${owner}` : "Promissory note",
  };
}

/** True for decisions whose options are cards from the player's own hand. */
export function isHandCardDecision(subtype: string | undefined): boolean {
  return (
    subtype !== undefined &&
    (NOTE_PICK_SUBTYPE.test(subtype) || DISCARD_SUBTYPES.has(subtype) || TAKE_SUBTYPES.has(subtype) || SECRET_SUBTYPES.has(subtype))
  );
}

/**
 * The card an option stands for: its name and printed text instead of a raw alias such as
 * "return dp". `null` when the decision is not about cards in hand.
 */
export function describeCardOption(
  subtype: string | undefined,
  option: Pick<ChoiceOptionDto, "id" | "label">,
): CardOptionInfo | null {
  if (!subtype) return null;
  if (NOTE_PICK_SUBTYPE.test(subtype)) return describeNote(option.id);
  if (DISCARD_SUBTYPES.has(subtype) || TAKE_SUBTYPES.has(subtype)) {
    const meta = findActionCardByName(option.label) ?? findActionCardMeta(option.id);
    return {
      title: meta?.name ?? option.label,
      text: meta?.description || null,
      badge: meta?.phase ? `${meta.phase} phase` : null,
    };
  }
  if (SECRET_SUBTYPES.has(subtype)) {
    const meta = findSecretObjectiveMeta(option.id);
    if (!meta) {
      return {
        title: humanizeId(option.id),
        text: null,
        badge: null,
      };
    }
    return {
      title: meta.name,
      text: meta.description || null,
      badge: `${meta.points} VP · scored in the ${meta.phase.toLowerCase()} phase`,
    };
  }
  return null;
}

/** One line under the question that says how many cards are held and what leaves. */
export function handDecisionNote(choice: Pick<PendingChoiceDto, "prompt" | "options" | "context">): string | null {
  const subtype = choice.context?.subtype;
  if (!subtype) return null;
  if (DISCARD_SUBTYPES.has(subtype)) {
    const held = /\bof (\d+)\s*$/.exec(choice.prompt)?.[1];
    return held
      ? `You hold ${held} action cards. Choose one to discard; it goes to the discard pile.`
      : "Choose an action card to discard.";
  }
  if (TAKE_SUBTYPES.has(subtype)) {
    return "Another player has shown you these action cards. Choose one to add to your hand; the rest stay with them.";
  }
  if (SECRET_SUBTYPES.has(subtype)) {
    return `You hold ${choice.options.length} secret objectives. Choose one to return to the deck; the others stay secret.`;
  }
  return null;
}

/** The confirm caption for a hand decision. */
export function handDecisionConfirmLabel(subtype: string | undefined): string | null {
  if (!subtype) return null;
  if (DISCARD_SUBTYPES.has(subtype)) return "Discard card";
  if (TAKE_SUBTYPES.has(subtype)) return "Take card";
  if (SECRET_SUBTYPES.has(subtype)) return "Return objective";
  return null;
}
