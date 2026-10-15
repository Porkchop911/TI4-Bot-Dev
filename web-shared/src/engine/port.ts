import { Host } from "./host.ts";
import type { FromWorker, HostPort, ToWorker } from "./protocol.ts";
import { HostRunner } from "./runner.ts";

/**
 * A port to a Worker that runs `worker.ts`, handed `module` at once. The page makes the Worker
 * itself, with a literal relative `new URL(".../web-shared/src/engine/worker.ts",
 * import.meta.url)`: vite bundles a Worker only from such a path. What the page posts before the
 * Worker is ready waits in its queue.
 */
export function workerPort(worker: Worker, module: WebAssembly.Module): HostPort {
  let listener: (message: FromWorker) => void = () => undefined;
  worker.onmessage = (event: MessageEvent<FromWorker>) => listener(event.data);
  worker.onerror = (event) => {
    event.preventDefault();
    listener({ t: "crash", message: event.message || "The engine worker failed" });
  };
  worker.postMessage({ t: "init", module } satisfies ToWorker);
  return {
    post: (message, transfer) => worker.postMessage(message, transfer ?? []),
    listen: (next) => {
      listener = next;
    },
    terminate: () => worker.terminate(),
  };
}

/**
 * A port that runs the engine in this thread, as the Worker does: each message in a task of its
 * own, in order. For a place without Workers (Node, the tests).
 */
export function inlinePort(module: WebAssembly.Module): HostPort {
  let listener: (message: FromWorker) => void = () => undefined;
  let closed = false;
  const later = (run: () => void) => {
    // A task, not a microtask: a message is not handled while the game runs or replays.
    const channel = new MessageChannel();
    channel.port1.onmessage = () => {
      channel.port1.close();
      if (!closed) {
        run();
      }
    };
    channel.port2.postMessage(null);
  };
  const runner = new HostRunner(
    (message) => later(() => listener(message)),
    (compiled) => Host.create(compiled),
  );
  void runner.handle({ t: "init", module });
  return {
    post: (message: ToWorker) =>
      later(() => {
        void runner.handle(message).catch((error: unknown) =>
          listener({
            t: "crash",
            message: error instanceof Error ? error.message : String(error),
          }),
        );
      }),
    listen: (next) => {
      listener = next;
    },
    terminate: () => {
      closed = true;
      void runner.handle({ t: "close" });
    },
  };
}
