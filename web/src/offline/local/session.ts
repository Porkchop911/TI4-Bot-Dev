import { engineHashOf, engineUrl } from "./engineAsset.ts";
import type { FromWorker, HostPort, ToWorker } from "./hostProtocol.ts";
import { LocalTransport, type LocalTransportOptions } from "./LocalTransport.ts";
import { sha256Hex } from "./sha256.ts";

/**
 * The lazily loaded half of the launcher: the engine file, the Web Worker and the transport.
 * Nothing here is part of the initial bundle; `launcher.ts` imports it when a game is opened.
 */

export interface LoadedEngine {
  module: WebAssembly.Module;
  /** Content hash of the engine file's name. */
  hash: string;
  /** sha-256 of the engine file itself (what saves and feedback name). */
  wasmSha256: string;
}

/** Which engine this is: build and content digest as the engine says, the file hash as measured. */
export interface EngineIdentity {
  build: string;
  contentDigest: string;
  wasmSha256: string;
}

let loading: Promise<LoadedEngine> | null = null;

/** Fetches and compiles the engine once per page (off the main thread). */
export function loadEngine(): Promise<LoadedEngine> {
  loading ??= (async () => {
    const url = await engineUrl();
    const { module, wasmSha256 } = await compile(url);
    return { module, hash: engineHashOf(url), wasmSha256 };
  })();
  loading.catch(() => {
    loading = null;
  });
  return loading;
}

async function compile(url: string): Promise<{ module: WebAssembly.Module; wasmSha256: string }> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`The engine could not be loaded (${response.status})`);
  // A server that has no such file answers with its start page (single-page fallback): this page
  // is from an older release than the server, and the engine file it names is gone.
  if (response.headers.get("content-type")?.startsWith("text/html"))
    throw new Error("This page is out of date: the engine file it needs is no longer on the server. Reload the page.");
  // The file is hashed while it compiles: the copy of the body is read for the hash.
  const hashing = response.clone().arrayBuffer().then(sha256Hex);
  // compileStreaming needs the wasm content type; a static host may not send it.
  if (response.headers.get("content-type")?.startsWith("application/wasm"))
    return { module: await WebAssembly.compileStreaming(response), wasmSha256: await hashing };
  const bytes = await response.arrayBuffer();
  return { module: await WebAssembly.compile(bytes), wasmSha256: await hashing };
}

const identity = new WeakMap<LoadedEngine, Promise<EngineIdentity>>();

/**
 * What the engine says about itself (`describe`: build and content digest) plus the file hash.
 * Asked once per loaded engine, in a short-lived worker, so it is known before a saved game is
 * replayed.
 */
export function engineIdentity(engine: LoadedEngine): Promise<EngineIdentity> {
  let found = identity.get(engine);
  if (!found) {
    found = (async () => {
      const port = await workerPort(engine);
      try {
        const described = await new Promise<{ engine_build?: string; content_digest?: string }>((resolve, reject) => {
          port.listen((message) => {
            if (message.t === "reply" && message.id === 1) {
              if (message.error !== undefined) reject(new Error(message.error));
              else resolve((message.value ?? {}) as { engine_build?: string; content_digest?: string });
            }
          });
          port.post({ t: "query", id: 1, request: { query: "describe" } });
        });
        return {
          build: described.engine_build ?? "unknown",
          contentDigest: described.content_digest ?? "unknown",
          wasmSha256: engine.wasmSha256,
        };
      } finally {
        port.terminate();
      }
    })();
    identity.set(engine, found);
    found.catch(() => identity.delete(engine));
  }
  return found;
}

/** A new Web Worker with the engine module handed in; resolves when it is ready. */
export function workerPort(engine: LoadedEngine): Promise<HostPort> {
  const worker = new Worker(new URL("./hostWorker.ts", import.meta.url), { type: "module" });
  let listener: (message: FromWorker) => void = () => undefined;
  const port: HostPort = {
    post: (message: ToWorker) => worker.postMessage(message),
    listen: (next) => {
      listener = next;
    },
    terminate: () => worker.terminate(),
  };
  return new Promise<HostPort>((resolve, reject) => {
    worker.onmessage = (event: MessageEvent<FromWorker>) => {
      if (event.data.t === "ready") {
        worker.onmessage = (next: MessageEvent<FromWorker>) => listener(next.data);
        resolve(port);
      } else if (event.data.t === "fatal") {
        worker.terminate();
        reject(new Error(event.data.message));
      }
    };
    worker.onerror = (event) => {
      worker.terminate();
      reject(new Error(event.message || "The engine worker failed to start"));
    };
    worker.postMessage({ t: "init", module: engine.module } satisfies ToWorker);
  });
}

/** A transport on a new worker per run (Retry and recovery open a new one). */
export function openTransport(
  engine: LoadedEngine,
  options: Omit<LocalTransportOptions, "createPort">,
): LocalTransport {
  return new LocalTransport({ ...options, createPort: () => workerPort(engine) });
}
