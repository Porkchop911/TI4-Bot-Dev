import type { Envelope, HostProgress, HostSave, StartConfig } from "./hostApi.ts";

/** Page to Worker. */
export type ToWorker =
  | { t: "init"; module: WebAssembly.Module }
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
  post(message: ToWorker): void;
  /** One listener; replaces the previous one. */
  listen(listener: (message: FromWorker) => void): void;
  terminate(): void;
}
