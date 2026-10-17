import { CONTENT } from "../protocol/catalogStore.ts";
import { humanizeId } from "../protocol/contentCatalog.ts";
import type { PlayerView } from "../protocol/types.ts";
import type { ActionToast } from "./actionToasts.ts";

interface LeaderRecord {
  name?: string;
  type?: string;
  abilityWindow?: string;
  abilityText?: string;
}

const LEADERS = CONTENT.leaders as unknown as Record<string, LeaderRecord>;

/** Each seat's leader statuses ("locked", "unlocked", "readied", ...), read from the public players. */
export function readLeaderStatuses(
  players: readonly Pick<PlayerView, "id" | "leaders">[] | null | undefined,
): Record<string, Record<string, string>> {
  const out: Record<string, Record<string, string>> = {};
  for (const player of players ?? []) {
    if (player?.id && player.leaders && typeof player.leaders === "object") out[player.id] = { ...player.leaders };
  }
  return out;
}

/** "Claire Gibson (commander): At the start of ...: You may place ..." from the content catalog. */
export function leaderAbilityLine(leaderId: string): { name: string; kind: string; text: string } {
  const meta = LEADERS[leaderId];
  const kind = meta?.type ?? (/(agent|commander|hero)$/.exec(leaderId)?.[1] ?? "leader");
  const name = meta?.name ?? humanizeId(leaderId.replace(/(agent|commander|hero)$/, " $1").trim());
  const text = [meta?.abilityWindow, meta?.abilityText].filter(Boolean).join(" ");
  return { name, kind, text };
}

/**
 * Toasts for commanders and heroes that became unlocked since `before` (null: the first state
 * seen, which is history, so none). The toast carries the printed ability, so the table learns
 * what the unlocked leader does (alpha feedback fb_1792039768).
 */
export function leaderUnlockToasts(
  before: Record<string, Record<string, string>> | null,
  after: Record<string, Record<string, string>>,
): ActionToast[] {
  if (!before) return [];
  const toasts: ActionToast[] = [];
  for (const [seat, leaders] of Object.entries(after)) {
    const prior = before[seat];
    if (!prior) continue;
    for (const [leaderId, status] of Object.entries(leaders)) {
      if (status !== "unlocked" || prior[leaderId] !== "locked") continue;
      const line = leaderAbilityLine(leaderId);
      if (line.kind !== "commander" && line.kind !== "hero") continue;
      toasts.push({
        id: `leader-unlocked:${seat}:${leaderId}`,
        actor: seat,
        category: "breakthrough",
        parts: [`unlocked the ${line.kind} ${line.name}${line.text ? `: ${line.text}` : ""}`],
        count: 1,
      });
    }
  }
  return toasts;
}
