import type { PendingChoiceDto } from "../protocol/types.ts";
import { findExplorationCardMeta } from "../protocol/contentCatalog.ts";

export interface ExploreCardView {
  name: string;
  /** Cultural, Industrial, Hazardous or Frontier. */
  type: string;
  /** The printed text, one paragraph per entry. */
  text: string[];
}

/**
 * The exploration card whose reward is being chosen (`<card>_choose_reward`), as printed. `null`
 * for any other decision, or when the card is unknown.
 */
export function describeExploreCard(
  choice: Pick<PendingChoiceDto, "context">,
): ExploreCardView | null {
  const subtype = choice.context?.subtype ?? "";
  if (!subtype.endsWith("_choose_reward")) return null;
  const source = choice.context?.source as Record<string, unknown> | undefined;
  const alias =
    typeof source?.Content === "string" ? source.Content : subtype.replace(/_choose_reward$/, "");
  const meta = findExplorationCardMeta(alias);
  if (!meta) return null;
  return {
    name: meta.name,
    type: meta.type,
    text: meta.description
      .split("\n")
      .map((line) => line.trim())
      .filter((line) => line.length > 0),
  };
}
