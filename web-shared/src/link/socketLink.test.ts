import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { HEARTBEAT_MS, type Socket } from "../net/socket.ts";
import type { LinkEvent } from "./link.ts";
import { type SocketLinkOptions, socketLink } from "./socketLink.ts";

/** A socket the test opens, feeds and closes by hand. */
class FakeSocket implements Socket {
  sent: unknown[] = [];
  closed = false;
  onopen: (() => void) | null = null;
  onmessage: ((event: { data: unknown }) => void) | null = null;
  onclose: ((event: { code?: number }) => void) | null = null;
  send(data: string): void {
    this.sent.push(JSON.parse(data));
  }
  close(): void {
    this.closed = true;
  }
  open(): void {
    this.onopen?.();
  }
  receive(data: unknown): void {
    this.onmessage?.({ data: typeof data === "string" ? data : JSON.stringify(data) });
  }
  lose(code?: number): void {
    this.onclose?.({ code });
  }
}

function start(options: Partial<SocketLinkOptions> = {}) {
  const sockets: FakeSocket[] = [];
  const link = socketLink(
    () => {
      const socket = new FakeSocket();
      sockets.push(socket);
      return socket;
    },
    { subscribe: () => ({ type: "subscribe", game_id: "g" }), ...options },
  );
  const events: LinkEvent[] = [];
  link.subscribe((event) => events.push(event));
  return { link, sockets, events, last: () => sockets[sockets.length - 1] as FakeSocket };
}

beforeEach(() => vi.useFakeTimers());
afterEach(() => vi.useRealTimers());

describe("socketLink", () => {
  it("subscribes on each socket that opens and tells it as opened", () => {
    const { sockets, events, last } = start({ stamp: { protocol_version: 9 } });
    expect(sockets).toHaveLength(1);
    last().open();
    expect(last().sent).toEqual([{ protocol_version: 9, type: "subscribe", game_id: "g" }]);
    expect(events).toEqual([{ kind: "opened" }]);
  });

  it("tells each message parsed and sends stamped messages", () => {
    const { link, events, last } = start({ stamp: { protocol_version: 9 } });
    last().open();
    last().receive({ type: "initial_snapshot" });
    link.send({ type: "submit_choice" });
    expect(events.slice(1)).toEqual([{ kind: "message", message: { type: "initial_snapshot" } }]);
    expect(last().sent[1]).toEqual({ protocol_version: 9, type: "submit_choice" });
  });

  it("sends nothing while no socket is open", () => {
    const { link, last } = start();
    link.send({ type: "submit_choice" });
    expect(last().sent).toEqual([]);
  });

  it("pings while a socket is open", () => {
    const { last } = start();
    last().open();
    vi.advanceTimersByTime(HEARTBEAT_MS * 2);
    expect(last().sent.slice(1)).toEqual([
      { type: "ping", sequence: 1 },
      { type: "ping", sequence: 2 },
    ]);
    const custom = start({ ping: (sequence) => ({ type: "ping", sequence: sequence * 10 }) });
    custom.last().open();
    vi.advanceTimersByTime(HEARTBEAT_MS);
    expect(custom.last().sent[1]).toEqual({ type: "ping", sequence: 10 });
  });

  it("tells a loss once as offline, and opens again after a growing wait", () => {
    const waits: number[] = [];
    const { sockets, events, last } = start({
      retryAfter: (attempt) => {
        waits.push(attempt);
        return 100 * (attempt + 1);
      },
    });
    last().open();
    last().lose(1006);
    expect(events).toEqual([{ kind: "opened" }, { kind: "offline" }]);
    vi.advanceTimersByTime(99);
    expect(sockets).toHaveLength(1);
    vi.advanceTimersByTime(1);
    expect(sockets).toHaveLength(2);
    last().lose();
    vi.advanceTimersByTime(200);
    expect(sockets).toHaveLength(3);
    expect(events).toHaveLength(2);
    expect(waits).toEqual([0, 1]);
    last().open();
    last().receive({ type: "initial_snapshot" });
    last().lose();
    expect(waits).toEqual([0, 1, 0]);
    expect(events.map((e) => e.kind)).toEqual([
      "opened",
      "offline",
      "opened",
      "message",
      "offline",
    ]);
  });

  it("lets the page set the wait of a loss and keep it quiet", () => {
    const { sockets, events, last } = start({
      lost: (event) => (event.code === 4001 ? { wait: 0, quiet: true } : undefined),
    });
    last().open();
    last().lose(4001);
    vi.advanceTimersByTime(0);
    expect(sockets).toHaveLength(2);
    expect(events).toEqual([{ kind: "opened" }]);
  });

  it("tells a message that is not JSON as fatal and stops", () => {
    const { sockets, events, last } = start();
    last().open();
    last().receive("{nope");
    expect(events.slice(1)).toEqual([{ kind: "fatal", message: expect.any(String) }]);
    expect(last().closed).toBe(true);
    vi.advanceTimersByTime(60_000);
    expect(sockets).toHaveLength(1);
  });

  it("closes the socket and tells nothing after close", () => {
    const { link, sockets, events, last } = start();
    last().open();
    link.close();
    expect(last().closed).toBe(true);
    vi.advanceTimersByTime(60_000);
    expect(sockets).toHaveLength(1);
    expect(events).toEqual([{ kind: "opened" }]);
  });
});
