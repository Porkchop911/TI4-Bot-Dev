/**
 * The kinds of reaction window a seat can declare it bluffs about.
 *
 * The ids are the server's (`crates/ti4-server/src/session/bluff.rs`, `TRIGGER_GROUPS`); a test
 * keeps the two lists equal. The labels group the engine's windows (the timing text printed on
 * the action cards) by what is happening at the table.
 */
export interface BluffTrigger {
  id: string;
  label: string;
}

export const BLUFF_TRIGGERS: readonly BluffTrigger[] = [
  { id: "action_card_played", label: "When an action card is played" },
  { id: "system_activated", label: "After a system is activated" },
  { id: "movement", label: "After ships move" },
  { id: "space_combat", label: "During a space combat" },
  { id: "ground_combat", label: "When an invasion or ground combat starts" },
  { id: "planet_control", label: "When a planet changes hands" },
  { id: "production", label: "After units are produced" },
  { id: "agenda", label: "During the agenda phase" },
  { id: "strategy_card", label: "Around strategy cards" },
  { id: "turn_end", label: "At the start or end of a turn" },
  { id: "transaction", label: "When a transaction is made" },
];

export const BLUFF_TRIGGER_IDS: readonly string[] = BLUFF_TRIGGERS.map((trigger) => trigger.id);

const key = (gameId: string, seat: string) => `bluff_triggers:${gameId}:${seat}`;

/** The seat's declaration kept in this browser; unknown ids and a broken value are dropped. */
export function readDeclaredTriggers(gameId: string, seat: string): string[] {
  try {
    const parsed: unknown = JSON.parse(localStorage.getItem(key(gameId, seat)) ?? "[]");
    if (!Array.isArray(parsed)) return [];
    return BLUFF_TRIGGER_IDS.filter((id) => parsed.includes(id));
  } catch {
    return [];
  }
}

export function writeDeclaredTriggers(gameId: string, seat: string, triggers: string[]): void {
  try {
    if (triggers.length === 0) localStorage.removeItem(key(gameId, seat));
    else localStorage.setItem(key(gameId, seat), JSON.stringify(triggers));
  } catch {
    // Storage unavailable: the declaration then only lasts until the page is closed.
  }
}
