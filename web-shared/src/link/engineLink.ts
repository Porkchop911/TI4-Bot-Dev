import type { Envelope, StartConfig } from "../engine/host.ts";
import type { FromWorker, HostPort, ModelFile } from "../engine/protocol.ts";
import type { ClientMessage, ServerMessage } from "../wire/index.ts";
import { type GameLink, type HostCommand, listeners } from "./link.ts";

export interface EngineLinkOptions {
  /**
   * Called with each question of the engine before anything of it is told (the replay of a save
   * that did not play back to where it was: `resume_check`). When it returns a promise the link
   * waits for it, and what the engine sends meanwhile waits too; when it returns nothing the
   * question is told at once. A throw or a rejection is told as `fatal`.
   */
  gate?(envelope: Envelope): void | Promise<void>;
  /**
   * The files of the MLP models that the game seats (`mlp:<name>`): the Worker loads them into
   * each instance of the game before it starts it. Their buffers are transferred: pass copies.
   */
  models?: ModelFile[];
}

/** A link to the engine: its commands have replies, and a page can wait for its next question. */
export interface EngineLink extends GameLink {
  /** A command (a `ClientMessage`, `save_state`, `history`, `submit_batch`, …): its reply. */
  request(command: ClientMessage | HostCommand): Promise<Envelope>;
  /** A query of the engine (`{"query": …}`): no game needs to run. */
  query(request: object): Promise<unknown>;
  /** The envelope of the next question (or end) the engine reaches; rejects when the link fails. */
  nextQuestion(): { promise: Promise<Envelope | null>; cancel(): void };
}

type Pending = { resolve: (value: unknown) => void; reject: (error: Error) => void };
type Waiter = { resolve: (envelope: Envelope | null) => void; reject: (error: Error) => void };

/**
 * The game on the engine behind `port` (a Worker that has its module, or a fake), started with
 * `config`. The messages of each reply and question are told as `message` events, in order; a
 * question is also told as a `save` (when it has one), and the first question or end as `opened`.
 * A crash, a fatal error and an end with an error are told as `fatal`. To play on after that, a
 * page makes a new port and a new link, from the last save.
 */
export function engineLink(
  port: HostPort,
  config: StartConfig,
  options: EngineLinkOptions = {},
): EngineLink {
  const all = listeners();
  const replies = new Map<number, Pending>();
  let waiters: Waiter[] = [];
  let nextId = 1;
  let opened = false;
  /** Why the link ended; nothing of the engine is told after it. */
  let ended: Error | null = null;
  /** A gate is open: what the engine sends waits behind it. */
  let behind: Promise<void> | null = null;

  const settle = (error: Error | null, envelope: Envelope | null = null) => {
    const waiting = waiters;
    waiters = [];
    for (const waiter of waiting) {
      if (error) {
        waiter.reject(error);
      } else {
        waiter.resolve(envelope);
      }
    }
  };
  const end = (error: Error) => {
    if (ended) {
      return;
    }
    ended = error;
    settle(error);
    for (const pending of replies.values()) {
      pending.reject(error);
    }
    replies.clear();
  };
  const fail = (message: string) => {
    if (ended) {
      return;
    }
    end(new Error(message));
    all.tell({ kind: "fatal", message });
  };
  const tell = (messages: readonly unknown[]) => {
    for (const message of messages) {
      all.tell({ kind: "message", message: message as ServerMessage });
    }
  };
  const open = () => {
    if (!opened) {
      opened = true;
      all.tell({ kind: "opened" });
    }
  };

  const ask = (envelope: Envelope, save: Extract<FromWorker, { t: "ask" }>["save"]) => {
    tell(envelope.messages);
    if (save) {
      all.tell({ kind: "save", save });
    }
    open();
    settle(null, envelope);
  };

  const handle = (message: FromWorker): Promise<void> | undefined => {
    if (ended) {
      return;
    }
    switch (message.t) {
      case "ready":
        return;
      case "fatal":
      case "crash":
        fail(message.message);
        return;
      case "progress":
        all.tell({ kind: "progress", done: message.progress.done, total: message.progress.total });
        return;
      case "reply": {
        const pending = replies.get(message.id);
        replies.delete(message.id);
        if (!pending) {
          return;
        }
        if (message.error !== undefined) {
          pending.reject(new Error(message.error));
        } else if (message.envelope) {
          tell(message.envelope.messages);
          pending.resolve(message.envelope);
        } else {
          pending.resolve(message.value);
        }
        return;
      }
      case "ask": {
        let held: unknown;
        try {
          held = options.gate?.(message.envelope);
        } catch (error) {
          fail(text(error));
          return;
        }
        if (!(held instanceof Promise)) {
          ask(message.envelope, message.save);
          return;
        }
        return held.then(
          () => {
            if (!ended) {
              ask(message.envelope, message.save);
            }
          },
          (error: unknown) => fail(text(error)),
        );
      }
      case "end": {
        if (message.save) {
          all.tell({ kind: "save", save: message.save });
        }
        tell(message.envelope.messages);
        const error = message.envelope.end?.error;
        if (error) {
          fail(error);
          return;
        }
        open();
        settle(null, message.envelope);
      }
    }
  };

  const receive = (message: FromWorker) => {
    const done = behind ? behind.then(() => handle(message)) : handle(message);
    if (done) {
      const mine: Promise<void> = done.then(() => {
        if (behind === mine) {
          behind = null;
        }
      });
      behind = mine;
    }
  };

  const post = <T>(make: (id: number) => Parameters<HostPort["post"]>[0]): Promise<T> => {
    if (ended) {
      return Promise.reject(ended);
    }
    const id = nextId++;
    return new Promise<T>((resolve, reject) => {
      replies.set(id, { resolve: resolve as (value: unknown) => void, reject });
      port.post(make(id));
    });
  };
  const request = (command: ClientMessage | HostCommand) =>
    post<Envelope>((id) => ({ t: "command", id, request: command }));

  port.listen(receive);
  if (options.models?.length) {
    port.post(
      { t: "models", models: options.models },
      options.models.map((model) => model.bytes),
    );
  }
  port.post({ t: "start", config });

  return {
    send(message) {
      request(message).catch(() => undefined);
    },
    request,
    query: (query) => post<unknown>((id) => ({ t: "query", id, request: query })),
    nextQuestion() {
      let waiter!: Waiter;
      const promise = new Promise<Envelope | null>((resolve, reject) => {
        waiter = { resolve, reject };
      });
      if (ended) {
        waiter.reject(ended);
      } else {
        waiters.push(waiter);
      }
      return { promise, cancel: () => (waiters = waiters.filter((w) => w !== waiter)) };
    },
    subscribe: all.add,
    close() {
      end(new Error("The link was closed"));
      all.clear();
      port.terminate();
    },
  };
}

const text = (error: unknown): string => (error instanceof Error ? error.message : String(error));
