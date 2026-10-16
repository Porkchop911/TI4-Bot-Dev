import type { EventVisibility } from "./types.ts";

export type VisibilityKind = EventVisibility["visibility"];

/**
 * Decodes an event's `visibility` field. The server serialises
 * `EventVisibility` (an internally tagged enum) as a field value, so the wire
 * form is an object: `{"visibility": "public"}` / `{"visibility": "seat",
 * "seat": "p1"}` / `{"visibility": "referee"}`. A plain string is accepted for
 * back-compat with older fixtures and recordings. Anything else reads as
 * "referee" (the most restrictive), so unknown shapes never leak.
 */
export function visibilityKind(value: unknown): VisibilityKind {
  const tag =
    typeof value === "string"
      ? value
      : value && typeof value === "object"
        ? (value as { visibility?: unknown }).visibility
        : undefined;
  return tag === "public" || tag === "seat" ? tag : "referee";
}

export const isPublicEvent = (e: { visibility: unknown }): boolean =>
  visibilityKind(e.visibility) === "public";
