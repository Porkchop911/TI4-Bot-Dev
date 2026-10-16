// What a seat has staged and not sent: the staged movement, landings and production, the
// private draft of a tactical action, and the draft of a secondary. The game keeps it for the
// seat (`ti4_host::staged`): the server for every tab and device of the seat, the engine of a
// local game in its saved game. Both speak `ti4_host::protocol::staged` (`seatSync`, used by
// `wsTransport.ts` and `wasmTransport.ts`).
import type { StagedSlot, StagedValue } from "./wire";

export type { StagedSlot } from "./wire";

export const SLOTS: StagedSlot[] = ["movement", "plan", "secondary"];

/** The largest value of a slot as JSON, in bytes: `MAX_STAGED_VALUE_BYTES` of the server. */
export const MAX_STAGED_BYTES = 48 * 1024;

/** The size of a value as the server measures it: its JSON, in UTF-8. */
export const stagedBytes = (value: unknown): number =>
  new TextEncoder().encode(JSON.stringify(value)).length;

/** Where the staged work of one seat is kept. The values are the page's; the store does not read them. */
export interface SeatStore {
  /** The value of a slot. Null: nothing is staged there. */
  get(slot: StagedSlot): unknown;
  /** Null clears the slot. A value equal to the one there changes nothing. */
  set(slot: StagedSlot, value: unknown): void;
  /**
   * Told when another tab or device of the seat changed a slot, with the new value: `get`
   * returns it from then on. `server`: the server wrote it, not a tab (a ready draft of a
   * secondary that no longer fits, or one that the server sent). A change of `set` is not told.
   */
  subscribe(listener: (slot: StagedSlot, value: unknown, server: boolean) => void): () => void;
}

/** JSON with the keys of every object in order: the server keeps the keys of a value sorted. */
function canonical(value: unknown): string {
  return JSON.stringify(value ?? null, (_, item: unknown) =>
    item && typeof item === "object" && !Array.isArray(item)
      ? Object.fromEntries(
          Object.entries(item as Record<string, unknown>).sort(([a], [b]) => (a < b ? -1 : 1)),
        )
      : item,
  );
}

/** Two values of a slot are the same, whatever the order of their keys. */
export const sameStaged = (a: unknown, b: unknown) => a === b || canonical(a) === canonical(b);

/** `SeatStagedMsg` of the server: the slots of the seat, after a change or after the snapshot. */
export interface SeatStaged {
  revision: number;
  slots: Partial<Record<StagedSlot, StagedValue>>;
  writer?: number;
  connection?: number;
}

/** What an older page kept for the seat in this browser: put once, when the server has nothing. */
export interface LegacyStaged {
  read(): Partial<Record<StagedSlot, unknown>>;
  drop(): void;
}

/** The writes wait this long for the next change: a movement is many clicks. */
export const STAGED_DEBOUNCE_MS = 250;
/** A write that the server did not answer in this time is given up. */
const ANSWER_MS = 5_000;

interface Slot {
  /** The revision of the slot as the server told it last (0: never written). */
  revision: number;
  /** The value of the slot at that revision. */
  server: unknown;
  /** The value that the page sees: the server's, or what the page set since. */
  local: unknown;
  /** `local` is not sent yet. */
  dirty: boolean;
  /** A write is on its way; the next one waits for its answer. */
  inFlight: boolean;
}

/**
 * The staged work of a seat on the server. `put` sends one write (`put_staged`) and says whether
 * it was sent; `receive` takes every `seat_staged`; `lost` is called when the socket is gone.
 *
 * - The id of the connection comes with the first `seat_staged` after the snapshot. Until then
 *   nothing is written.
 * - One write of a slot is on its way at a time: the next one waits for the echo (the message
 *   with `writer` = this connection, which carries the new revision), and goes on that base.
 * - A value equal to the server's is not written: the server would answer nothing.
 * - A message with a newer revision of a slot, from another connection or from none, is the
 *   server's value. It replaces the slot, also a value that was not sent yet. One from no
 *   connection (and not the first after the snapshot) is a write of the server itself.
 * - The message after a snapshot (with `connection`) is what the seat has: it replaces every
 *   slot whose revision differs. The engine of a local game sends it when the seat that the page
 *   is shown as changes; its revisions count up for the whole page.
 * - A value over `MAX_STAGED_BYTES` stays in this page and is not written.
 */
export function seatSync(
  put: (slot: StagedSlot, base: number, value: unknown) => boolean,
  legacy?: LegacyStaged,
) {
  const slots = Object.fromEntries(
    SLOTS.map((slot) => [
      slot,
      { revision: 0, server: null, local: null, dirty: false, inFlight: false },
    ]),
  ) as Record<StagedSlot, Slot>;
  const listeners = new Set<(slot: StagedSlot, value: unknown, server: boolean) => void>();
  const debounce = new Map<StagedSlot, ReturnType<typeof setTimeout>>();
  const answer = new Map<StagedSlot, ReturnType<typeof setTimeout>>();
  /** The id of this connection. Null while there is none. */
  let me: number | null = null;
  let migrated = false;

  const tell = (slot: StagedSlot, server = false) => {
    for (const listener of listeners) {
      listener(slot, slots[slot].local, server);
    }
  };
  const settle = (slot: StagedSlot) => {
    slots[slot].inFlight = false;
    clearTimeout(answer.get(slot));
    answer.delete(slot);
  };
  const flush = (slot: StagedSlot) => {
    const at = slots[slot];
    if (me === null || at.inFlight || !at.dirty || debounce.has(slot)) {
      return;
    }
    at.dirty = false;
    if (sameStaged(at.local, at.server) || stagedBytes(at.local) > MAX_STAGED_BYTES) {
      return;
    }
    // In flight before it is sent: the engine of a local game answers within `put`.
    at.inFlight = true;
    if (!put(slot, at.revision, at.local ?? null)) {
      at.inFlight = false;
      at.dirty = true;
      return;
    }
    if (!at.inFlight) {
      // The echo came already.
      return;
    }
    answer.set(
      slot,
      setTimeout(() => {
        answer.delete(slot);
        at.inFlight = false;
        flush(slot);
      }, ANSWER_MS),
    );
  };

  const store: SeatStore = {
    get: (slot) => slots[slot].local,
    set(slot, value) {
      const at = slots[slot];
      at.local = value ?? null;
      at.dirty = true;
      clearTimeout(debounce.get(slot));
      debounce.set(
        slot,
        setTimeout(() => {
          debounce.delete(slot);
          flush(slot);
        }, STAGED_DEBOUNCE_MS),
      );
    },
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };

  return {
    store,
    /** A write is on its way: an error of the server now may be its refusal. */
    waiting: () => SLOTS.some((slot) => slots[slot].inFlight),
    receive(message: SeatStaged) {
      if (message.connection !== undefined) {
        me = message.connection;
      }
      const own = me !== null && message.writer === me;
      for (const slot of SLOTS) {
        const at = slots[slot];
        const told = message.slots[slot] ?? { revision: 0 };
        if (own && told.revision === message.revision && at.inFlight) {
          // The echo of this write: its revision is the base of the next one.
          settle(slot);
          at.revision = told.revision;
          at.server = told.value ?? null;
        } else if (
          told.revision > at.revision ||
          (message.connection !== undefined && told.revision !== at.revision)
        ) {
          settle(slot);
          at.revision = told.revision;
          at.server = told.value ?? null;
          at.dirty = false;
          if (!sameStaged(at.local, at.server)) {
            at.local = at.server;
            tell(slot, message.writer === undefined && message.connection === undefined);
          }
        } else if (message.writer === undefined && at.inFlight) {
          // The answer to a write that the server did not take, with nothing newer: what was
          // not written is not tried again until the page changes it.
          settle(slot);
        }
      }
      if (!migrated && message.connection !== undefined) {
        migrated = true;
        const old = legacy?.read() ?? {};
        const kept = (slot: StagedSlot) => old[slot] !== undefined && old[slot] !== null;
        if (message.revision === 0 && SLOTS.some(kept)) {
          for (const slot of SLOTS) {
            if (kept(slot)) {
              slots[slot].local = old[slot];
              slots[slot].dirty = true;
              tell(slot);
            }
          }
        }
        legacy?.drop();
      }
      for (const slot of SLOTS) {
        flush(slot);
      }
    },
    /** Writes what waits for the next change now: the page is left or hidden. */
    flushNow() {
      for (const slot of SLOTS) {
        if (debounce.has(slot)) {
          clearTimeout(debounce.get(slot));
          debounce.delete(slot);
          flush(slot);
        }
      }
    },
    /** The connection is gone. A write that was on its way is sent again on the next one, if it still applies. */
    lost() {
      me = null;
      for (const slot of SLOTS) {
        if (slots[slot].inFlight) {
          slots[slot].dirty = true;
        }
        settle(slot);
      }
    },
    close() {
      for (const timer of [...debounce.values(), ...answer.values()]) {
        clearTimeout(timer);
      }
      debounce.clear();
      answer.clear();
      listeners.clear();
    },
  };
}
