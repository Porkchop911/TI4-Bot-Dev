import { afterEach, describe, expect, it, vi } from "vitest";
import { PreviewUnsupportedError, StaleChoiceError } from "../../protocol/client.ts";
import { LocalTransport } from "./LocalTransport.ts";
import { GAME, FakePort, accepted, envelope, question, rejected, save, snapshot } from "./testKit.ts";
import type { StartConfig } from "@ti4/shared/engine/host.ts";

const config: StartConfig = { setup: { game_id: GAME, seed: 7, players: 3 } };

async function open() {
  const ports: FakePort[] = [];
  const saves: unknown[] = [];
  const progress: unknown[] = [];
  const transport = new LocalTransport({
    gameId: GAME,
    nickname: "Tester",
    config,
    createPort: async () => {
      const port = new FakePort();
      ports.push(port);
      return port;
    },
    onSave: (s) => void saves.push(s),
    onProgress: (p) => progress.push(p),
  });
  await transport.begin();
  const port = ports[0];
  port.emit({ t: "ask", envelope: envelope("ask", { messages: [snapshot(1, "n1")] }), save: save([]) });
  await transport.ready;
  return { transport, port, ports, saves, progress };
}

afterEach(() => vi.useRealTimers());

describe("LocalTransport", () => {
  it("starts the engine with the config and shows the first question", async () => {
    const { transport, port, saves } = await open();
    expect(port.posted[0]).toEqual({ t: "start", config });
    const state = transport.getState();
    expect(state.status).toBe("connected");
    expect(state.pendingChoice?.nonce).toBe("n1");
    expect(state.gameVersion).toBe(1);
    expect(saves).toHaveLength(1);
    expect(transport.capabilities).toMatchObject({ turnRedo: true, multiplayer: false, splicePreview: false });
    expect(transport.nickname).toBe("Tester");
  });

  it("shows a game whose engine names it otherwise (an imported game, a game of web2)", async () => {
    const ports: FakePort[] = [];
    const transport = new LocalTransport({
      gameId: "off_imported",
      nickname: "Tester",
      config,
      createPort: async () => {
        const port = new FakePort();
        ports.push(port);
        return port;
      },
    });
    await transport.begin();
    ports[0].emit({ t: "ask", envelope: envelope("ask", { messages: [snapshot(1, "n1")] }), save: save([]) });
    await transport.ready;
    expect(transport.getState().lastError).toBeNull();
    expect(transport.getState().pendingChoice?.nonce).toBe("n1");
  });

  it("answers a choice with the nonce and version and settles at the next question", async () => {
    const { transport, port } = await open();
    port.onCommand = (request, id) => ({
      t: "reply",
      id,
      envelope: envelope("command", { messages: [accepted(1, String(request.option_id))], resume: 0 }),
    });
    let done = false;
    const promise = transport.submitChoice("o1").then(() => (done = true));
    await Promise.resolve();
    await Promise.resolve();
    expect(port.commands("submit_choice")[0]).toMatchObject({
      type: "submit_choice",
      protocol_version: 4,
      game_id: GAME,
      nonce: "n1",
      expected_version: 1,
      option_id: "o1",
    });
    await new Promise((r) => setTimeout(r, 0));
    expect(done).toBe(false);
    port.emit({ t: "ask", envelope: envelope("ask", { messages: [question(2, "n2")] }), save: save(["o1"]) });
    await promise;
    expect(transport.getState().pendingChoice?.nonce).toBe("n2");
    expect(transport.latestSave()?.answers).toEqual(["o1"]);
  });

  // Nightly 2026-10-14 #8 (o1-43-2146 and 10 other offline runs): the lone `strategic` answer of
  // the previous question went out with the next question's nonce.
  it("drops an answer stamped with the nonce of an earlier question and sends nothing", async () => {
    const { transport, port } = await open();
    port.onCommand = (request, id) => ({
      t: "reply",
      id,
      envelope: envelope("command", { messages: [accepted(1, String(request.option_id))], resume: 0 }),
    });
    const first = transport.submitChoice("o1", "n1");
    await new Promise((r) => setTimeout(r, 0));
    port.emit({ t: "ask", envelope: envelope("ask", { messages: [question(2, "n2")] }), save: save(["o1"]) });
    await first;
    await expect(transport.submitChoice("o1", "n1")).rejects.toBeInstanceOf(StaleChoiceError);
    await new Promise((r) => setTimeout(r, 0));
    expect(port.commands("submit_choice")).toHaveLength(1);
    expect(transport.getState().lastError).toBeNull();
  });

  it("shares a repeated submit of the same option and refuses another option for the same decision", async () => {
    const { transport, port } = await open();
    port.onCommand = (_r, id) => ({ t: "reply", id, envelope: envelope("command", { messages: [accepted(1, "o1")], resume: 0 }) });
    const first = transport.submitChoice("o1");
    const again = transport.submitChoice("o1");
    await expect(transport.submitChoice("o2")).rejects.toThrow(/still pending/);
    await new Promise((r) => setTimeout(r, 0));
    port.emit({ t: "ask", envelope: envelope("ask", { messages: [question(2, "n2")] }), save: save(["o1"]) });
    await Promise.all([first, again]);
    expect(port.commands("submit_choice")).toHaveLength(1);
  });

  it("rejects a refused choice with the engine's reason", async () => {
    const { transport, port } = await open();
    port.onCommand = (_r, id) => ({ t: "reply", id, envelope: envelope("command", { messages: [rejected(1)] }) });
    await expect(transport.submitChoice("o1")).rejects.toThrow(/Stale decision nonce/);
    expect(transport.getState().lastError).toMatch(/Stale decision nonce/);
  });

  it("runs a batch: refused at once, applied, and interrupted by a reaction window", async () => {
    const { transport, port } = await open();
    const plan = { kind: "tactical_movement" as const, destination: "27", steps: [{ kind: "done_moving" as const }] };
    port.onCommand = (_r, id) => ({
      t: "reply",
      id,
      envelope: envelope("command", { batch: { status: "refused", applied: 0, message: "no such ship" } }),
    });
    await expect(transport.submitBatch(plan)).rejects.toThrow("Batch rejected: no such ship");

    port.onCommand = (_r, id) => ({ t: "reply", id, envelope: envelope("command", { resume: 3 }) });
    const applied = transport.submitBatch(plan);
    await new Promise((r) => setTimeout(r, 0));
    port.emit({
      t: "ask",
      envelope: envelope("ask", { messages: [question(2, "n2")], batch: { status: "applied", applied: 1 } }),
      save: save(["a"]),
    });
    await applied;
    expect(transport.getState().batchResume ?? null).toBeNull();

    const interrupted = transport.submitBatch({ ...plan });
    await new Promise((r) => setTimeout(r, 0));
    port.emit({
      t: "ask",
      envelope: envelope("ask", {
        messages: [question(3, "n3")],
        batch: { status: "interrupted", applied: 1, remaining_steps: [{ kind: "done_moving" }], waiting: { subtype: "reaction_x", own_seat: true } },
      }),
      save: save(["a", "b"]),
    });
    await interrupted;
    expect(transport.getState().batchResume).toMatchObject({ applied: 1, waiting: { subtype: "reaction_x", ownSeat: true } });
    transport.dismissBatchResume();
    expect(transport.getState().batchResume).toBeNull();
  });

  it("rejects a batch whose first step the engine refuses to apply later (rolled back)", async () => {
    const { transport, port } = await open();
    port.onCommand = (_r, id) => ({ t: "reply", id, envelope: envelope("command", { resume: 0 }) });
    const promise = transport.submitBatch({ kind: "tactical_movement", destination: "1", steps: [] });
    await new Promise((r) => setTimeout(r, 0));
    port.emit({
      t: "ask",
      envelope: envelope("ask", { messages: [snapshot(2, "n9", 0)], batch: { status: "refused", applied: 0, message: "step 2 failed" } }),
      save: save([]),
    });
    await expect(promise).rejects.toThrow("Batch rejected: step 2 failed");
  });

  it("undoes and rewinds through the history command", async () => {
    const { transport, port } = await open();
    port.onCommand = (request, id) => ({
      t: "reply",
      id,
      envelope: envelope("command", request.action === "undo" ? { resume: -2 } : { resume: -1, restart: { answers: [], redo: [], generation: 1, version: 5 } }),
    });
    const undo = transport.changeHistory("undo");
    await new Promise((r) => setTimeout(r, 0));
    expect(port.commands("history")[0]).toEqual({ type: "history", action: "undo" });
    port.emit({ t: "ask", envelope: envelope("ask", { messages: [snapshot(6, "n5", 1)] }), save: save([]) });
    await undo;
    expect(transport.getState().history.generation).toBe(1);

    const rewind = transport.changeHistory({ cursor: 4 });
    await new Promise((r) => setTimeout(r, 0));
    expect(port.commands("history")[1]).toEqual({ type: "history", action: { restore_cursor: { cursor: 4 } } });
    const byEvent = transport.changeHistory({ eventId: "e7" });
    await new Promise((r) => setTimeout(r, 0));
    expect(port.commands("history")[2]).toEqual({ type: "history", action: { restore: { event_id: "e7" } } });
    port.emit({ t: "ask", envelope: envelope("ask", { messages: [snapshot(8, "n6", 2)] }), save: save([]) });
    await Promise.all([rewind, byEvent]);
  });

  it("undoes the last turn through the history command and ignores the redo steps of a server", async () => {
    const { transport, port } = await open();
    port.onCommand = (_r, id) => ({
      t: "reply",
      id,
      envelope: envelope("command", { resume: -1, restart: { answers: [], redo: [], generation: 1, version: 5 } }),
    });
    const undo = transport.turnRedoCommand({ action: "request", turns: 2 });
    await new Promise((r) => setTimeout(r, 0));
    expect(port.commands("history")[0]).toEqual({ type: "history", action: { undo_turn: { turns: 2 } } });
    port.emit({ t: "ask", envelope: envelope("ask", { messages: [snapshot(6, "n5", 1)] }), save: save([]) });
    await undo;
    expect(transport.getState().history.generation).toBe(1);
    await transport.turnRedoCommand({ action: "autoplay" });
    await transport.turnRedoCommand({ action: "restore" });
    expect(port.commands("history")).toHaveLength(1);
  });

  it("rejects a history change the engine refuses", async () => {
    const { transport, port } = await open();
    port.onCommand = (_r, id) => ({ t: "reply", id, envelope: envelope("command", { messages: [rejected(1)] }) });
    await expect(transport.changeHistory("redo")).rejects.toThrow(/Rejected/);
  });

  it("answers who-can-go-here from reach_into and has no secondary preview, turn redo or splice", async () => {
    const { transport, port } = await open();
    port.onCommand = (request, id) => ({
      t: "reply",
      id,
      envelope: envelope("command", {
        messages: [
          {
            type: "reach_into",
            protocol_version: 4,
            game_id: GAME,
            request_id: request.request_id as number,
            system_id: String(request.system_id),
            as_of_version: 1,
            origins: [],
          },
        ],
      }),
    });
    await expect(transport.queryReach("27")).resolves.toEqual({ kind: "reach", systemId: "27", asOfVersion: 1, origins: [] });
    await expect(transport.previewSecondary("c", "p", [])).rejects.toBeInstanceOf(PreviewUnsupportedError);
    await expect(transport.fetchTurnRedoStatus()).resolves.toBeNull();
    await expect(transport.turnRedoCommand({ action: "keep" })).resolves.toBeUndefined();
    expect((transport as { previewSplice?: unknown }).previewSplice).toBeUndefined();
  });

  it("sets a reaction mode and shows the engine's state update", async () => {
    const { transport, port } = await open();
    port.onCommand = (_r, id) => ({
      t: "reply",
      id,
      envelope: envelope("command", { messages: [{ ...question(2, "n1"), reaction_modes: { Shields: "never" } }] }),
    });
    transport.setReactionMode("Shields", "never");
    await new Promise((r) => setTimeout(r, 0));
    expect(port.commands("set_reaction_mode")[0]).toMatchObject({ card: "Shields", mode: "never", game_id: GAME });
    expect(transport.getState().snapshot?.reaction_modes).toEqual({ Shields: "never" });
    // Bluff holds do not exist against bots: nothing is sent.
    transport.setReactionIntent();
    transport.passReactionHold();
    expect(port.posted.filter((m) => m.t === "command")).toHaveLength(1);
  });

  it("exports the replay from the latest save", async () => {
    const { transport } = await open();
    const replay = await transport.fetchReplay();
    expect(replay.filename).toBe(`ti4-offline-${GAME}.json`);
    expect(JSON.parse(replay.text).seed).toBe(7);
  });

  it("reports replay progress and a save that does not play back", async () => {
    const ports: FakePort[] = [];
    const progress: unknown[] = [];
    const transport = new LocalTransport({
      gameId: GAME,
      config: { setup: {}, resume: save(["a"]) },
      createPort: async () => (ports[ports.length] = new FakePort()),
      onProgress: (p) => progress.push(p),
    });
    await transport.begin();
    ports[0].emit({ t: "progress", progress: { done: 20, total: 40 } });
    expect(progress).toEqual([{ done: 20, total: 40 }]);
    ports[0].emit({
      t: "ask",
      envelope: envelope("ask", { messages: [snapshot(1, "n1")], resume_check: { ok: false } }),
      save: save(["a"]),
    });
    await expect(transport.ready).rejects.toThrow(/engine has changed/);
    expect(transport.getState().fatal?.kind).toBe("gone");
  });

  it("shows a crash as a fatal state and Retry plays the last save again in a new engine", async () => {
    const { transport, port, ports } = await open();
    port.emit({ t: "ask", envelope: envelope("ask", { messages: [question(2, "n2")] }), save: save(["o1", "o2"]) });
    port.emit({ t: "crash", message: "unreachable executed" });
    expect(transport.getState().fatal?.message).toMatch(/unreachable executed/);
    expect(transport.getState().status).toBe("error");
    transport.retryNow();
    await new Promise((r) => setTimeout(r, 0));
    expect(ports).toHaveLength(2);
    expect(ports[0].terminated).toBe(true);
    expect(ports[1].posted[0]).toMatchObject({ t: "start", config: { resume: { answers: ["o1", "o2"] } } });
    ports[1].emit({ t: "ask", envelope: envelope("ask", { messages: [snapshot(3, "n7", 0)] }), save: save(["o1", "o2"]) });
    expect(transport.getState().status).toBe("connected");
    expect(transport.getState().fatal).toBeNull();
  });

  it("ends the game: game over messages reach the reducer, a failed end is an error", async () => {
    const { transport, port } = await open();
    port.emit({
      t: "end",
      envelope: envelope("end", {
        messages: [{ type: "game_over", protocol_version: 4, game_id: GAME, game_version: 9, winner: "a", final_scores: { a: 10 } }],
        end: { finished: true, left: false, error: null, decisions: 9, answers: 4 },
      }),
      save: save(["x"], true),
    });
    expect(transport.getState().turnStatus).toEqual({ kind: "game_over", winner: "a" });
    expect(transport.latestSave()?.finished).toBe(true);
    port.emit({ t: "end", envelope: envelope("end", { end: { finished: false, left: false, error: "boom", decisions: 0, answers: 0 } }), save: null });
    expect(transport.getState().fatal?.message).toMatch(/boom/);
  });

  it("closes the engine shortly after the page stops using it, unless it comes back", async () => {
    vi.useFakeTimers();
    const { transport, port } = await open();
    transport.stop();
    transport.start();
    vi.advanceTimersByTime(5_000);
    expect(port.terminated).toBe(false);
    transport.stop();
    vi.advanceTimersByTime(2_000);
    expect(port.terminated).toBe(true);
  });
});
