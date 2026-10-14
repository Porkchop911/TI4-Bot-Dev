/**
 * Plain-language wording for the corner toast of a decision the engine took because only one
 * option was legal. Built on the client from the note's own fields (prompt, chosen label, reason);
 * the engine's prompt text is a question ("choose a strategy card"), which reads badly as a
 * statement, so known prompts get a sentence of their own and everything else a generic one.
 */
const KNOWN: Array<{ match: RegExp; text: (chosen: string) => string }> = [
  // The last card of the strategy draft (prompt "choose a strategy card" / "Strategy Card").
  { match: /strategy card/i, text: (c) => `Only one strategy card was left: you took ${c}` },
  { match: /^production payment plan/i, text: (c) => `Paid from your payment plan: ${c}` },
  { match: /^pay \d+ more /i, text: (c) => `Only one way left to pay: ${c}` },
  { match: /^discard an action card/i, text: (c) => `Only one action card to discard: ${c}` },
  { match: /^discard a secret objective/i, text: (c) => `Only one secret objective to discard: ${c}` },
];

/** Reasons the wording above already says, so they are not repeated in brackets. */
const GENERIC_REASON = "the only legal option";

export function autoResolveText(prompt: string, chosen: string, reason?: string): string {
  const known = KNOWN.find((k) => k.match.test(prompt.trim()));
  if (known) return known.text(chosen);
  const why = reason && reason !== GENERIC_REASON ? ` (${reason})` : "";
  return `Only one choice: ${chosen}${why}`;
}
