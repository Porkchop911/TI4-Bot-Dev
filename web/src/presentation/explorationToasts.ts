import { findAttachmentMeta, findExplorationCardMeta, findPlanetMeta, humanizeId } from "../protocol/contentCatalog.ts";
import type { ActionToast } from "./actionToasts.ts";

/** One entry of the engine's `exploration_log` (public: exploration cards are turned face up). */
export interface ExplorationRecordDto {
  round?: number;
  player: string;
  deck: string;
  card: string;
  planet?: string | null;
  outcome?: string;
  system?: string | null;
  result?: string;
  /** Signed holdings the card changed (`trade_goods`, `commodities`, `command_tokens`, ...). */
  gains?: Record<string, number>;
  /** What the card put on the map or in the play area (`wormhole:GAMMA:25`, `ion_storm:..`, ...). */
  placed?: string[];
}

/** Cards that cost an infantry: declining leaves nothing gained. */
const SACRIFICE_CARDS = /^(cm|exp|vfs)\d$/;

export function readExplorationLog(state: Record<string, unknown> | null | undefined): ExplorationRecordDto[] {
  const raw = state?.exploration_log;
  if (!Array.isArray(raw)) return [];
  return raw.filter(
    (r): r is ExplorationRecordDto =>
      !!r && typeof r === "object" && typeof (r as ExplorationRecordDto).player === "string",
  );
}

const GAIN_LABELS: Record<string, [string, string]> = {
  trade_goods: ["trade good", "trade goods"],
  commodities: ["commodity", "commodities"],
  command_tokens: ["command token", "command tokens"],
  action_cards: ["action card", "action cards"],
  secret_objectives: ["secret objective", "secret objectives"],
  relics: ["relic", "relics"],
  technologies: ["technology", "technologies"],
};

/** "gained 2 trade goods, spent 1 command token" from the record's structured `gains`. */
export function gainsText(gains: Record<string, number> | undefined): string {
  const parts: string[] = [];
  for (const [key, amount] of Object.entries(gains ?? {})) {
    if (!amount) continue;
    const [one, many] = GAIN_LABELS[key] ?? [humanizeId(key).toLowerCase(), humanizeId(key).toLowerCase()];
    const n = Math.abs(amount);
    parts.push(`${amount > 0 ? "gained" : "lost"} ${n} ${n === 1 ? one : many}`);
  }
  return parts.join(", ");
}

/** What the card put on the board, from the structured `placed` tags ("placed a gamma wormhole in 25"). */
export function placedText(placed: readonly string[] | undefined): string {
  const parts: string[] = [];
  for (const tag of placed ?? []) {
    const [kind, a, b] = tag.split(":");
    if (kind === "wormhole" && a && b) parts.push(`placed a ${a.toLowerCase()} wormhole in system ${b}`);
    else if (kind === "ion_storm" && a) parts.push(`placed the ion storm in system ${a}${b ? ` (${b.toLowerCase()} side)` : ""}`);
    else if (kind === "planet" && a) parts.push(`discovered ${findPlanetMeta(a)?.name ?? humanizeId(a)}${b ? ` in system ${b}` : ""}`);
  }
  return parts.join(", ");
}

/** "attached Mining World" / "took a cultural relic fragment" / "gained 1 trade good" ... */
export function explorationResultText(record: ExplorationRecordDto): string {
  const outcome = record.outcome ?? "";
  const result = (record.result ?? "").trim();
  if (outcome.startsWith("attached:")) {
    const id = outcome.slice("attached:".length);
    const meta = findAttachmentMeta(id);
    const gains = [
      meta?.resourcesModifier ? `${meta.resourcesModifier > 0 ? "+" : ""}${meta.resourcesModifier} resources` : "",
      meta?.influenceModifier ? `${meta.influenceModifier > 0 ? "+" : ""}${meta.influenceModifier} influence` : "",
    ].filter(Boolean);
    return `attached ${meta?.name ?? humanizeId(id)}${gains.length ? ` (${gains.join(", ")})` : ""}`;
  }
  // The engine's own sentence wins; structured gains/placed fill in for records without one and
  // add what went onto the board, which the sentence does not always say.
  const placed = placedText(record.placed);
  const text = result || gainsText(record.gains);
  if (text) return placed && !text.includes(placed) ? `${text}, ${placed}` : text;
  if (placed) return placed;
  if (SACRIFICE_CARDS.test(record.card)) return "no infantry removed, nothing gained";
  // A card that stays in the play area (Enigmatic Device) gains nothing now but is not "nothing".
  const cardText = findExplorationCardMeta(record.card)?.description ?? "";
  if (/place this card faceup in your play area/i.test(cardText)) return "kept faceup in the play area for later";
  return "nothing gained";
}

/** The one-line summary of a finished exploration, without the actor's name. */
export function explorationSummary(record: ExplorationRecordDto): string {
  const card = findExplorationCardMeta(record.card)?.name ?? humanizeId(record.card);
  const trait = (record.deck || "").toLowerCase();
  const where = record.planet
    ? `${findPlanetMeta(record.planet)?.name ?? humanizeId(record.planet)} (${trait})`
    : record.system
      ? `frontier token in system ${record.system}`
      : "a frontier token";
  return `explored ${where}: ${card} - ${explorationResultText(record)}`;
}

/**
 * Toasts for exploration records the viewer has not been told about. `from` is how many records
 * were already known; records still without an outcome (a card waiting for a decision) are not
 * reported yet and do not advance the cursor. Returns the toasts and the new cursor.
 */
export function explorationToasts(
  log: readonly ExplorationRecordDto[],
  from: number,
): { toasts: ActionToast[]; next: number } {
  const toasts: ActionToast[] = [];
  let next = Math.min(from, log.length);
  for (let i = next; i < log.length; i++) {
    const record = log[i];
    if (!record.outcome) {
      // Only the newest record can still be waiting for its decision; an older one that never got
      // an outcome must not hold back every toast after it.
      if (i === log.length - 1) break;
      next = i + 1;
      continue;
    }
    toasts.push({
      id: `explore:${i}:${record.player}:${record.card}`,
      actor: record.player,
      category: "explore",
      parts: [explorationSummary(record)],
      count: 1,
    });
    next = i + 1;
  }
  return { toasts, next };
}
