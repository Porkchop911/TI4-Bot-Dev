import type { MapChoice } from "../../protocol/types.ts";
import type { OfflineMapSource } from "../types.ts";
import type { FromWorker, HostPort } from "@ti4/shared/engine/protocol.ts";
import { loadEngine, workerPort, type LoadedEngine } from "./session.ts";

export interface MapSourceDeps {
  load: () => Promise<LoadedEngine>;
  port: (engine: LoadedEngine) => Promise<HostPort>;
  /** How long the query worker lives after its last question. */
  idleMs: number;
}

/**
 * The picker's data from the engine: `describe` for the layouts of every table size (asked once),
 * `map_preview` for the pictures. One short-lived worker answers all questions; it is ended after
 * `idleMs` without one, so a person who only looks at the form keeps no second engine alive.
 */
export function createMapSource(deps: Partial<MapSourceDeps> = {}): OfflineMapSource {
  const { load = loadEngine, port: open = workerPort, idleMs = 20_000 } = deps;
  let port: Promise<HostPort> | null = null;
  let current: HostPort | null = null;
  let timer: ReturnType<typeof setTimeout> | undefined;
  let next = 1;
  const waiting = new Map<number, { resolve: (value: unknown) => void; reject: (error: Error) => void }>();
  let described: Promise<Record<string, unknown>> | null = null;

  const end = () => {
    current?.terminate();
    current = null;
    port = null;
    for (const entry of waiting.values()) entry.reject(new Error("The engine worker ended"));
    waiting.clear();
  };

  const ask = async (request: object): Promise<unknown> => {
    clearTimeout(timer);
    port ??= (async () => {
      const opened = await open(await load());
      opened.listen((message: FromWorker) => {
        if (message.t !== "reply") return;
        const entry = waiting.get(message.id);
        waiting.delete(message.id);
        if (!entry) return;
        if (message.error !== undefined) entry.reject(new Error(message.error));
        else entry.resolve(message.value);
      });
      current = opened;
      return opened;
    })();
    const worker = await port.catch((error) => {
      port = null;
      throw error;
    });
    const id = next++;
    try {
      return await new Promise<unknown>((resolve, reject) => {
        waiting.set(id, { resolve, reject });
        worker.post({ t: "query", id, request });
      });
    } finally {
      clearTimeout(timer);
      if (waiting.size === 0) timer = setTimeout(end, idleMs);
    }
  };

  const describe = () => {
    described ??= ask({ query: "describe" }) as Promise<Record<string, unknown>>;
    described.catch(() => {
      described = null;
    });
    return described;
  };

  return {
    async catalog(playerCount) {
      const maps = ((await describe()).maps ?? {}) as Record<string, unknown>;
      return maps[String(playerCount)] ?? [];
    },
    async preview(choice: MapChoice, playerCount, variant) {
      return ask({ query: "map_preview", choice, player_count: playerCount, variant });
    },
  };
}
