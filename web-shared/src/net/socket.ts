// A websocket of the server that is opened again when it is lost. web2's socket of a game
// (`wsTransport.ts`) and socket of a table (`table.ts`) use it.

/** The part of `WebSocket` that is used, so a test can pass a socket of its own. */
export interface Socket {
  send(data: string): void;
  close(): void;
  onopen: (() => void) | null;
  onmessage: ((event: { data: unknown }) => void) | null;
  onclose: ((event: { code?: number }) => void) | null;
}

/** A websocket at `url`, as a connection uses one. */
export function openSocket(url: string): Socket {
  const real = new WebSocket(url);
  const socket: Socket = {
    send: (data) => real.send(data),
    close: () => real.close(),
    onopen: null,
    onmessage: null,
    onclose: null,
  };
  real.onopen = () => socket.onopen?.();
  real.onmessage = (event) => socket.onmessage?.(event);
  real.onclose = (event) => socket.onclose?.(event);
  return socket;
}

/** The server is at the address of the page, as the page itself: a socket at `path` of it. */
export const pageSocket = (path: string): Socket =>
  openSocket(`${location.protocol === "https:" ? "wss" : "ws"}://${location.host}${path}`);

/** How long to wait before try `attempt` (from 0): 0.5 s, then twice as long, up to 15 s. */
export const backoff = (attempt: number) => Math.min(15_000, 500 * 2 ** attempt);

/** The server counts a player as present while its connection pings, and closes a silent socket. */
export const HEARTBEAT_MS = 10_000;

export interface Connection {
  /** False when the socket is not open: nothing was sent. */
  send(text: string): boolean;
  /** Drops the socket and opens one now. */
  again(): void;
  /** Ends the connection. Nothing is told after it. */
  close(): void;
}

/**
 * Keeps one socket open. `opened` is told for each socket that opens (the subscription is sent
 * there), `beat` every `HEARTBEAT_MS` while it is open, and `lost` when it ends: its answer is
 * how long to wait before the next socket, in ms.
 */
export function connect(
  open: () => Socket,
  on: {
    opened(): void;
    message(text: string): void;
    beat(): void;
    /** `opened` is false for a socket that never opened: the server may have refused it. */
    lost(event: { code?: number; opened: boolean }): number;
  },
): Connection {
  let socket: Socket | null = null;
  let ready = false;
  let closed = false;
  let retry: ReturnType<typeof setTimeout> | null = null;
  let beat: ReturnType<typeof setInterval> | null = null;

  const drop = () => {
    if (retry !== null) {
      clearTimeout(retry);
    }
    if (beat !== null) {
      clearInterval(beat);
    }
    retry = beat = null;
    ready = false;
    const old = socket;
    socket = null;
    if (old) {
      old.onopen = old.onmessage = old.onclose = null;
      old.close();
    }
  };
  const start = () => {
    if (closed) {
      return;
    }
    const made = open();
    socket = made;
    made.onopen = () => {
      if (socket !== made) {
        return;
      }
      ready = true;
      on.opened();
      beat = setInterval(on.beat, HEARTBEAT_MS);
    };
    made.onmessage = (event) => {
      if (socket === made) {
        on.message(String(event.data));
      }
    };
    made.onclose = (event) => {
      if (socket !== made) {
        return;
      }
      const opened = ready;
      drop();
      const wait = on.lost({ code: event.code, opened });
      if (!closed) {
        retry = setTimeout(() => {
          retry = null;
          start();
        }, wait);
      }
    };
  };
  start();

  return {
    send(text) {
      if (!socket || !ready) {
        return false;
      }
      socket.send(text);
      return true;
    },
    again() {
      drop();
      start();
    },
    close() {
      closed = true;
      drop();
    },
  };
}
