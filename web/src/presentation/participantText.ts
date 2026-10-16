import type { LobbyDto } from "../protocol/types.ts";
import { playerDisplay } from "./playerDisplay.ts";

// Only standalone participant tokens in prose are presentation references. In
// particular, a token inside a composite machine/content ID is not a name.
const PARTICIPANT_TOKEN = /(^|[^\w:/|.-])(player_[a-zA-Z0-9_]+)(?!\.[\w])(?=$|[^\w:/|-])/g;
const GENERATED_ID = /^player_[a-f0-9]{64}$/;

/** Split prose into literal text and resolvable participant references. */
export function participantReferences(
  text: string,
  lobby: LobbyDto | null,
): (string | { id: string })[] {
  const known = new Set(
    lobby?.slots.flatMap((slot) => (slot.occupant ? [slot.occupant] : [])) ?? [],
  );
  const parts: (string | { id: string })[] = [];
  let cursor = 0;
  for (const match of text.matchAll(PARTICIPANT_TOKEN)) {
    const id = match[2];
    if (!known.has(id) && !GENERATED_ID.test(id)) continue;
    const start = match.index + match[1].length;
    if (start > cursor) parts.push(text.slice(cursor, start));
    parts.push({ id });
    cursor = start + id.length;
  }
  if (cursor < text.length) parts.push(text.slice(cursor));
  return parts;
}

// An engine event type that leaked into prose ("after TACTICAL_ACTION_ENDED"): two or more
// UPPER_SNAKE words. Composite machine ids are lower case or carry `:`/`|`, so they never match.
const EVENT_TYPE = /(?<![\w:/|.-])[A-Z][A-Z0-9]*(?:_[A-Z0-9]+)+(?![\w:/|-])/g;

/** "TACTICAL_ACTION_ENDED" -> "tactical action ended". */
export function readableEventTypes(text: string): string {
  return text.replace(EVENT_TYPE, (type) => type.toLowerCase().replaceAll("_", " "));
}

export function participantText(
  text: string,
  lobby: LobbyDto | null,
  seatingOrder: readonly string[],
): string {
  return participantReferences(readableEventTypes(text), lobby)
    .map((part) =>
      typeof part === "string" ? part : playerDisplay(lobby, seatingOrder, part.id).label,
    )
    .join("");
}
