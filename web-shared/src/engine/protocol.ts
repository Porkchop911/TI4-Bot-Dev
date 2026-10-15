import type { Envelope, HostProgress, HostSave, StartConfig } from "./host.ts";

/**
 * The file of an MLP model that the games of the Worker seat: its bytes (a transferred buffer),
 * and the SHA-256 of the whole file that the page expects (`GET /api/models`).
 */
export interface ModelFile {
  sha256: string;
  bytes: ArrayBuffer;
}

/** Page to Worker. */
export type ToWorker =
  | { t: "init"; module: WebAssembly.Module }
  /** The models of the games that start from now on: each instance loads them before its game. */
  | { t: "models"; models: ModelFile[] }
  | { t: "query"; id: number; request: object }
  | { t: "start"; config: StartConfig }
  | { t: "command"; id: number; request: object }
  | { t: "close" };

/** Worker to page. */
export type FromWorker =
  | { t: "ready" }
  /** The engine cannot run here (no JSPI) or its module could not be instantiated. */
  | { t: "fatal"; message: string }
  | { t: "reply"; id: number; envelope?: Envelope; value?: unknown; error?: string }
  /** The game asks the person. `save` is the game as it stands at the question. */
  | { t: "ask"; envelope: Envelope; save: HostSave | null }
  | { t: "progress"; progress: HostProgress }
  /** The run ended (the game is over, or it was left). `save` is the final state of a finished game. */
  | { t: "end"; envelope: Envelope; save: HostSave | null }
  /** The engine trapped or failed in a way the game cannot go on from. */
  | { t: "crash"; message: string };

/** Where the page talks to the engine: a Web Worker, or a fake in the tests. */
export interface HostPort {
  /** `transfer`: the buffers of the message that move to the Worker (the files of `models`). */
  post(message: ToWorker, transfer?: Transferable[]): void;
  /** One listener; replaces the previous one. */
  listen(listener: (message: FromWorker) => void): void;
  terminate(): void;
}
