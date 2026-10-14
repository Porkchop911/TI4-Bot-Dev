import { findBreakthroughForFaction, findBreakthroughMeta, humanizeId } from "../protocol/contentCatalog.ts";
import type { BreakthroughMeta } from "../protocol/contentCatalog.ts";
import type { ActionToast } from "./actionToasts.ts";

/** A seat's breakthrough as the player sheet and faction card show it. */
export interface BreakthroughLine {
  id: string;
  name: string;
  /** "Biotic + Warfare". */
  synergy: string;
  text: string | null;
  unlocked: boolean;
}

/** Each seat's earned breakthrough alias (or null), read from the state's players. */
export function readBreakthroughs(state: Record<string, unknown> | null | undefined): Record<string, string | null> {
  const players = state?.players;
  const out: Record<string, string | null> = {};
  if (!Array.isArray(players)) return out;
  for (const p of players) {
    if (!p || typeof p !== "object") continue;
    const { id, breakthrough } = p as { id?: unknown; breakthrough?: unknown };
    if (typeof id === "string") out[id] = typeof breakthrough === "string" && breakthrough ? breakthrough : null;
  }
  return out;
}

export function synergyText(card: Pick<BreakthroughMeta, "synergy">): string {
  const colors = card.synergy.filter((color) => color.toUpperCase() !== "NONE");
  return colors.length ? colors.map((color) => humanizeId(color.toLowerCase())).join(" + ") : "none";
}

/**
 * The breakthrough card of a faction, with whether the seat holds it. `held` is the alias the state
 * says the seat owns; the printed card is public, so a locked one still shows its text.
 */
export function describeBreakthrough(
  faction: string | null | undefined,
  held: string | null | undefined,
): BreakthroughLine | null {
  const card = findBreakthroughMeta(held) ?? findBreakthroughForFaction(faction);
  if (!card) return null;
  return {
    id: card.id,
    name: card.name,
    synergy: synergyText(card),
    text: card.text ?? null,
    unlocked: Boolean(held),
  };
}

/** The note a decision that grants the viewer's breakthrough carries: name, synergy and printed text. */
export function breakthroughNote(faction: string | null | undefined): string | null {
  const line = describeBreakthrough(faction, null);
  if (!line) return null;
  return `Grants your breakthrough, ${line.name} (synergy ${line.synergy}): ${line.text ?? ""}`.trim();
}

/** Toasts for seats whose breakthrough appeared since `before` (null: first state seen, so none). */
export function breakthroughToasts(
  before: Record<string, string | null> | null,
  after: Record<string, string | null>,
): ActionToast[] {
  if (!before) return [];
  const toasts: ActionToast[] = [];
  for (const [seat, alias] of Object.entries(after)) {
    if (!alias || before[seat] === alias) continue;
    const card = findBreakthroughMeta(alias);
    toasts.push({
      id: `breakthrough:${seat}:${alias}`,
      actor: seat,
      category: "breakthrough",
      parts: [
        `unlocked the breakthrough ${card?.name ?? humanizeId(alias)}${card ? ` (synergy ${synergyText(card)})` : ""}`,
      ],
      count: 1,
    });
  }
  return toasts;
}
