/// <reference lib="webworker" />
import { Host, hasJspi } from "./host.ts";
import { HostRunner } from "./runner.ts";
import type { FromWorker, ToWorker } from "./protocol.ts";

// The Web Worker that owns the engine. The wasm stack is parked here (JSPI) while the person
// decides, so the page's main thread is never blocked by the engine and never has to wait for it.
const scope = self as unknown as DedicatedWorkerGlobalScope;
const post = (message: FromWorker) => scope.postMessage(message);

const runner = new HostRunner(post, (module) => Host.create(module));

scope.onmessage = (event: MessageEvent<ToWorker>) => {
  if (event.data.t === "init" && !hasJspi()) {
    post({
      t: "fatal",
      message: "This browser cannot run the engine (WebAssembly JSPI is missing).",
    });
    return;
  }
  void runner
    .handle(event.data)
    .catch((error: unknown) =>
      post({ t: "crash", message: error instanceof Error ? error.message : String(error) }),
    );
};
