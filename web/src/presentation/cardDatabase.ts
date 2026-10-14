import { GENERATED_CONTENT_CATALOG } from "../protocol/generatedContentManifest.ts";

/**
 * Card database with descriptions for Action Cards, Promissory Notes, and other tradeable items.
 * This provides UI-side descriptions for cards in trade offers.
 */

interface CardInfo {
  name: string;
  description: string;
  phase?: string;
  window?: string;
  type?: string;
}

const ACTION_CARDS: Record<string, CardInfo> = {
  abs: {
    name: "Ancient Burial Sites",
    type: "Action Card",
    phase: "Agenda",
    window: "At the start of the agenda phase",
    description: "Choose 1 player. Exhaust each cultural planet owned by that player.",
  },
  assassin: {
    name: "Assassinate Representative",
    type: "Action Card",
    phase: "Agenda",
    window: "After an agenda is revealed",
    description: "Choose 1 player. That player cannot vote on this agenda.",
  },
  bribery: {
    name: "Bribery",
    type: "Action Card",
    phase: "Agenda",
    window: "After the speaker votes on an agenda",
    description:
      "Spend any number of trade goods. For each trade good spent, cast 1 additional vote for the outcome on which you voted.",
  },
  bunker: {
    name: "Bunker",
    type: "Action Card",
    phase: "Action",
    window: "At the start of an invasion",
    description: "During this invasion, apply -4 to the result of each BOMBARDMENT roll against planets you control.",
  },
  confusing: {
    name: "Confusing Legal Text",
    type: "Action Card",
    phase: "Agenda",
    window: "When you are elected as the outcome of an agenda",
    description: "Choose 1 player. That player is the elected player instead.",
  },
};

const PROMISSORY_NOTES: Record<string, CardInfo> = {
  ceasefire: {
    name: "Ceasefire",
    type: "Promissory Note",
    description:
      "After the player activates a system that contains 1 or more of your units: That player cannot move units to the active system.",
  },
  political_secret: {
    name: "Political Secret",
    type: "Promissory Note",
    description:
      "When an agenda is revealed: That player cannot vote, play action cards, or use faction abilities until after the agenda has been resolved.",
  },
  support_for_throne: {
    name: "Support for the Throne",
    type: "Promissory Note",
    description:
      "When you receive this card, you must place it faceup in your play area and gain 1 victory point. Return it and lose the point when that player activates a system with their units.",
  },
  trade_agreement: {
    name: "Trade Agreement",
    type: "Promissory Note",
    description: "At the end of the status phase, gain 1 trade good. Then return this card to the player.",
  },
};

const SPECIAL_ITEMS: Record<string, CardInfo> = {
  fragment: {
    name: "Relic Fragment",
    type: "Special",
    description: "A valuable relic fragment that can be traded or used for strategic advantage.",
  },
  secret_objective: {
    name: "Secret Objective",
    type: "Special",
    description: "A secret goal card that grants victory points when conditions are met.",
  },
};

/**
 * Get card information by name or alias
 */
export function getCardInfo(cardName: string): CardInfo | null {
  if (!cardName) return null;

  const normalized = cardName.toLowerCase().replace(/\s+/g, "_");

  return ACTION_CARDS[normalized] || ACTION_CARDS[cardName] || PROMISSORY_NOTES[normalized] || PROMISSORY_NOTES[cardName] || SPECIAL_ITEMS[normalized] || SPECIAL_ITEMS[cardName] || catalogCardInfo(normalized);
}

type CatalogRecord = { name: string; text?: string; description?: string; phase?: string; window?: string };
const CATALOG_ACTION_CARDS = GENERATED_CONTENT_CATALOG.actionCards as unknown as Record<string, CatalogRecord>;
const CATALOG_SECRETS = GENERATED_CONTENT_CATALOG.secretObjectives as unknown as Record<string, CatalogRecord>;
const CATALOG_NOTES = GENERATED_CONTENT_CATALOG.promissoryNotes as unknown as Record<string, CatalogRecord>;

/** Engine ids the local table lacks: note aliases (`cf`, generic notes are `<color>_cf`) and action cards (`sabo1`). */
function catalogCardInfo(id: string): CardInfo | null {
  const note = CATALOG_NOTES[`<color>_${id}`] || CATALOG_NOTES[id];
  if (note)
    return {
      name: note.name,
      type: "Promissory Note",
      description: (note.text ?? "").replace(/<color>/g, "issuing"),
    };
  const card = CATALOG_ACTION_CARDS[id] || CATALOG_ACTION_CARDS[id.replace(/\d+$/, "")];
  if (card)
    return {
      name: card.name,
      type: "Action Card",
      phase: card.phase,
      window: card.window,
      description: card.description ?? "",
    };
  const secret = CATALOG_SECRETS[id];
  if (secret)
    return {
      name: secret.name,
      type: "Secret Objective",
      phase: secret.phase,
      description: secret.description ?? "",
    };
  return null;
}

/**
 * Parse a card name from various formats
 */
export function parseCardName(label: string): string {
  // Extract card name from patterns like "Action Card: Bribery for 2 TG"
  if (label.includes(":")) {
    const parts = label.split(":");
    if (parts.length > 1) {
      return parts[1].split(" for ")[0].trim();
    }
  }
  return label;
}

/**
 * Get description for a card by name or label
 */
export function getCardDescription(label: string): string | null {
  const cardName = parseCardName(label);
  const info = getCardInfo(cardName);
  return info ? info.description : null;
}

/**
 * Get full card info for display
 */
export function getCardDisplayInfo(label: string): CardInfo | null {
  const cardName = parseCardName(label);
  return getCardInfo(cardName);
}
