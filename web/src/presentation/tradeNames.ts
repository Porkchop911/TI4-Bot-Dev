/**
 * Printed names for the card ids the engine puts in trade text (`cf:generic`, `ra:jolnar`,
 * `support_for_throne`, `sabo1`). One lookup shared by the trade items, the staging summary, the
 * Quick deals and the answer prompt, so no raw id reaches the player.
 */
import { GENERATED_CONTENT_CATALOG } from "../protocol/generatedContentManifest.ts";
import { getCardInfo } from "./cardDatabase.ts";

export type TradeCardKind = "note" | "action" | "secret";

const KIND_TYPE: Record<TradeCardKind, string> = {
  note: "Promissory Note",
  action: "Action Card",
  secret: "Secret Objective",
};

const SECRETS = GENERATED_CONTENT_CATALOG.secretObjectives as unknown as Record<string, { name: string }>;

const titleCase = (s: string): string =>
  s
    .split(/[\s_:]+/)
    .filter(Boolean)
    .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
    .join(" ");

export interface PrintedName {
  /** The text to show. */
  name: string;
  /** False when the id is not in any card table and the name is a cleaned-up guess. */
  known: boolean;
}

/**
 * The printed name of a card id. Notes are `alias:owner`: a non-generic owner is kept
 * (`ceasefire:sol` becomes "Ceasefire (sol)"), `:generic` is dropped. An unknown id gets a readable
 * fallback (underscores and colons to spaces, title case) and `known: false`.
 */
export function printedCardName(kind: TradeCardKind, rawId: string): PrintedName {
  const id = rawId.trim();
  const [head, ...rest] = kind === "note" ? id.split(":") : [id];
  const owner = rest.join(":").split(" for ")[0].trim();
  const suffix = owner && owner !== "generic" ? ` (${owner})` : "";
  let known: string | undefined;
  if (kind === "secret") known = SECRETS[head]?.name;
  else {
    const info = getCardInfo(head);
    if (info && info.type === KIND_TYPE[kind]) known = info.name;
  }
  if (known) return { name: `${known}${suffix}`, known: true };
  return { name: `${titleCase(head)}${suffix}` || id, known: false };
}

const PREFIX_KIND: Record<string, TradeCardKind> = {
  "Promissory Note": "note",
  "Action Card": "action",
  "Secret Objective": "secret",
};

/** Rewrite a `Promissory Note: cf:generic`-style item label to the printed name; others pass through. */
export function printedItemLabel(label: string): string {
  const m = label.match(/^(Promissory Note|Action Card|Secret Objective):\s*(.+)$/);
  return m ? printedCardName(PREFIX_KIND[m[1]], m[2]).name : label;
}

const escapeRe = (s: string): string => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/**
 * Replace the given ids in engine text with printed names. Ids only match as whole tokens (not
 * inside a word, not part of a longer `a:b` id), longest first.
 */
export function replaceCardIds(
  text: string,
  ids: readonly { kind: TradeCardKind; id: string | undefined }[],
): string {
  let out = text;
  const list = ids
    .filter((x): x is { kind: TradeCardKind; id: string } => !!x.id)
    .sort((a, b) => b.id.length - a.id.length);
  for (const { kind, id } of list) {
    const re = new RegExp(`(?<![\\w:])${escapeRe(id)}(?![\\w:])`, "g");
    out = out.replace(re, () => printedCardName(kind, id).name);
  }
  return out;
}

/**
 * Printed names inside an answer prompt ("`Faction` gives `terms` for `terms` -- accept?"). Terms
 * are trade goods, commodities, relic fragments, "the action card X", "the secret objective X" or a
 * bare note id. A prompt of another shape is returned unchanged.
 */
export function humanizeOfferPrompt(prompt: string): string {
  const suffix = prompt.match(/\s+--\s+accept\?\s*$/)?.[0] ?? "";
  const body = prompt.slice(0, prompt.length - suffix.length);
  const at = body.indexOf(" gives ");
  if (at <= 0) return prompt;
  const rest = body.slice(at + " gives ".length);
  const cut = rest.indexOf(" for ");
  if (cut < 0) return prompt;
  const part = (raw: string): string => {
    const p = raw.trim();
    let m: RegExpMatchArray | null;
    if (/^\d+ (trade goods|commodities|relic fragments)$/.test(p) || p === "nothing") return p;
    if ((m = p.match(/^the action card (.+)$/))) return `the action card ${printedCardName("action", m[1]).name}`;
    if ((m = p.match(/^the secret objective (.+)$/)))
      return `the secret objective ${printedCardName("secret", m[1]).name}`;
    return p ? printedCardName("note", p).name : p;
  };
  const side = (t: string): string => t.split(", ").map(part).join(", ");
  return `${body.slice(0, at + " gives ".length)}${side(rest.slice(0, cut))} for ${side(rest.slice(cut + 5))}${suffix}`;
}
