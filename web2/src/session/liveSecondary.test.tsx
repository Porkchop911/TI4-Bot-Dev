import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import type { BlockView, FlowActionView, Intent, SeatRowView } from "../model";
import type { SaveStorage } from "./savedGame";
import { type SeatStore, type StagedSlot, localSeatStore } from "./seatStore";
import { type LiveSession, useLiveSession } from "./useLiveSession";
import { type Engine, createWasmTransport } from "./wasmTransport";
import type { SessionUpdate } from "./wire";

// The private draft of a secondary, on the real engine (`scripts/build-wasm.sh`): a game of
// seed 4 with seat a on this side. The other seats wait for a step before each decision, so
// the page is shown the game while another seat resolves its primary. A seat plays Leadership
// first, which asks seat a nothing; then another seat plays Politics.
// In the DOM project of the tests a module has no file address: the tests run from `web2/`.
const WASM = resolve(process.cwd(), "src/session/ti4.wasm");
const built = existsSync(WASM);

function storage(): SaveStorage & { values: Map<string, string> } {
  const values = new Map<string, string>();
  return {
    values,
    getItem: (key) => values.get(key) ?? null,
    setItem: (key, value) => void values.set(key, value),
    removeItem: (key) => void values.delete(key),
  };
}

/**
 * `server`: the transport keeps the staged work of the seat (`Transport.staged`), as a game on
 * the server does, so the page sends no ready draft itself.
 */
async function open({ server = false } = {}) {
  const engine: Engine = {
    module: await WebAssembly.compile(readFileSync(WASM)),
    hash: "test",
    scenarios: [],
    catalog: null,
  };
  const kept = storage();
  const local = localSeatStore(kept, {
    movement: "movement",
    plan: "plan",
    secondary: "secondary",
  });
  // The store of a seat that another tab or the server changes, as the server tells it (`emit`).
  const listeners = new Set<(slot: StagedSlot, value: unknown, server: boolean) => void>();
  const drafts: SeatStore = {
    get: local.get,
    set: vi.fn(local.set),
    subscribe(listener) {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
  };
  const emit = (slot: StagedSlot, value: unknown, byServer = false) =>
    act(async () => {
      local.set(slot, value);
      for (const listener of listeners) {
        listener(slot, value, byServer);
      }
    });
  const engineTransport = createWasmTransport(
    engine,
    { seed: 4, players: 8, humans: 1 },
    { stepping: true },
  );
  const transport = server ? { ...engineTransport, staged: drafts } : engineTransport;
  const applyDraft = vi.spyOn(transport, "applyDraft");
  const submitChoice = vi.spyOn(transport, "submitChoice");
  /** The strategic action of the last update of the game. */
  let last: SessionUpdate | null = null;
  transport.subscribe((event) => {
    if (event.kind === "update") {
      last = event.update;
    }
  });
  const { result } = renderHook(() => useLiveSession(transport, drafts));
  const flow = (live: LiveSession = result.current): FlowActionView => {
    const action = live.session?.view.action;
    if (action?.kind !== "flow") {
      throw new Error("the panel is not a list");
    }
    return action;
  };
  const dispatch = (intent: Intent) => act(async () => result.current.session!.dispatch(intent));
  const button = (label: string) => flow().footer.actions.find((item) => item.label === label);
  /** The list of the seats, when the panel is a strategic action. */
  const seats = (): SeatRowView[] | null => {
    const block = flow().blocks.find((item) => item.kind === "seats");
    return block?.kind === "seats" ? block.rows : null;
  };
  const mine = () => seats()?.find((row) => row.you) ?? null;
  const menu = (blocks: BlockView[] | undefined) =>
    (blocks ?? []).flatMap((block) => (block.kind === "menu" ? block.rows : []));
  /** Answers the open choice of the game with the option of this id. */
  const answer = async (option: string) => {
    await dispatch({ type: "chooseOption", option });
    await dispatch({ type: "flow", action: "resolve" });
  };
  /** Lets the other seats decide, one step at a time, until `reached`. */
  const stepUntil = async (reached: () => boolean) => {
    for (let steps = 0; steps < 400; steps += 1) {
      await waitFor(() => expect(reached() || !!button("Step")).toBe(true));
      if (reached()) {
        return;
      }
      await dispatch({ type: "simulate" });
    }
    throw new Error("the game did not come there");
  };
  await waitFor(() => expect(result.current.session).not.toBeNull());
  // Seat a takes the last card: the other seats act before it.
  await waitFor(() => expect(button("Send")).toBeDefined());
  await answer("pok8imperial");
  return {
    result,
    flow,
    dispatch,
    button,
    seats,
    mine,
    menu,
    stepUntil,
    applyDraft,
    submitChoice,
    kept,
    transport,
    drafts,
    emit,
    last: () => last,
  };
}

it.skipIf(!built)(
  "drafts a secondary while the primary is resolved, and sends the ready draft when the seat is asked",
  async () => {
    const game = await open();
    const { flow, dispatch, button, mine, menu, stepUntil, kept } = game;

    // Leadership asks seat a nothing, as the game is: its row is no draft.
    await stepUntil(() => flow().heading === "Leadership" && !!mine()?.text);
    expect(mine()).toMatchObject({
      status: { sign: "·" },
      text: "Nothing to decide as the game is now",
    });
    expect(button("Mark draft ready")).toBeUndefined();

    // Politics is played: the row of the viewer is the draft, before the primary is resolved.
    await stepUntil(() => flow().heading === "Politics" && !!mine()?.open);
    const rows = game.seats()!;
    expect(rows[0]).toMatchObject({ order: "P", status: { sign: "▶", label: "Deciding" } });
    expect(rows.filter((row) => row.open)).toHaveLength(1);
    expect(mine()?.status).toEqual({ tone: "draft", sign: "✎", label: "Private draft" });
    expect(flow().pill).toEqual({ tone: "draft", label: "Private draft" });
    // The question is not one of the game: the toolbar says who the game waits for.
    expect(game.result.current.session?.view.toolbar.status).not.toBe("Your decision");
    // It says what the secondary costs, in game values.
    expect(mine()?.open?.[0]).toMatchObject({ kind: "note", strong: "Strategy pool 2 → 1" });
    const options = menu(mine()?.open);
    expect(options.map((row) => row.title)).toEqual(["Decline", "Draw"]);
    expect(button("Record")?.disabled).toBe(true);

    // What is staged stays while another seat acts, and so does the main button.
    await dispatch(options[0].intent);
    expect(button("Record")?.disabled).toBe(false);
    expect(button("Step")).toBeDefined();
    const before = game.result.current.session;
    await dispatch({ type: "simulate" });
    await waitFor(() => expect(game.result.current.session).not.toBe(before));
    expect(button("Record")?.disabled).toBe(false);
    expect(menu(mine()?.open).map((row) => row.title)).toEqual(["Decline", "Draw"]);

    // An answer is recorded, not sent: the game is asked again, and the secondary is over.
    await dispatch({ type: "flow", action: "resolve" });
    await waitFor(() => expect(button("Mark draft ready")?.disabled).toBeFalsy());
    expect(mine()).toMatchObject({ status: { label: "Private draft" }, text: "Decline" });
    expect(game.submitChoice).toHaveBeenCalledTimes(1);

    // It starts over with nothing answered.
    await dispatch({ type: "startOver" });
    await waitFor(() => expect(menu(mine()?.open)).toHaveLength(2));
    await dispatch(options[0].intent);
    await dispatch({ type: "flow", action: "resolve" });
    await waitFor(() => expect(button("Mark draft ready")?.disabled).toBeFalsy());

    // A ready draft is closed and says what it will do. It is kept for a reload.
    await dispatch({ type: "flow", action: "ready" });
    expect(mine()).toMatchObject({ status: { sign: "✎", label: "Draft ready" } });
    expect(mine()?.text).toMatch(/^Decline · not sent, resolves after /);
    expect(mine()?.open).toBeUndefined();
    expect(JSON.parse(kept.values.get("secondary")!)).toMatchObject({
      card: "pok3politics",
      ready: true,
      answers: [{ label: "decline" }],
    });
    await dispatch({ type: "flow", action: "change" });
    expect(mine()?.status.label).toBe("Private draft");
    await dispatch({ type: "flow", action: "ready" });

    // The game comes to the seat: the draft is sent, and the seat is not asked.
    const asked: string[] = [];
    await stepUntil(() => {
      asked.push(flow().pill?.label ?? "");
      return mine()?.status.label === "Passed" || game.seats() === null;
    });
    expect(asked).not.toContain("Your decision");
    expect(game.applyDraft).toHaveBeenCalledTimes(1);
    expect(game.applyDraft.mock.calls[0][1].plan).toBeNull();
    expect(game.submitChoice).toHaveBeenCalledTimes(1);
    expect(kept.values.has("secondary")).toBe(false);
    game.transport.close();
  },
  120_000,
);

it.skipIf(!built)(
  "a draft that follows ends at the draw, and a draft that is not ready can still be sent",
  async () => {
    const game = await open();
    const { flow, dispatch, button, mine, menu, stepUntil } = game;
    const seatA = () => game.result.current.session!.view.players.rows.find((row) => row.isMe)!;
    await stepUntil(() => flow().heading === "Politics" && !!mine()?.open);
    const before = seatA();

    // The draw is hidden: the draft ends there, and the game goes on from there in Live.
    await dispatch(menu(mine()?.open)[1].intent);
    await dispatch({ type: "flow", action: "resolve" });
    await waitFor(() => expect(button("Mark draft ready")?.disabled).toBeFalsy());
    expect(mine()?.open).toEqual([
      {
        kind: "note",
        tone: "success",
        text: "The draft ends where the game would show something new.",
      },
    ]);
    // Nothing of the draft is in the game.
    expect(seatA().tokens).toEqual(before.tokens);

    // The seat is reached with the draft not marked ready: the game asks, and the draft can
    // be sent as it is.
    await stepUntil(() => flow().pill?.label === "Your decision");
    expect(mine()?.status).toMatchObject({ sign: "▶", label: "Your decision" });
    expect(button("Send")).toBeDefined();
    expect(button("Send draft")).toBeDefined();
    await dispatch({ type: "confirmApply" });
    await stepUntil(() => mine()?.status.label === "Followed" || game.seats() === null);
    expect(game.applyDraft).toHaveBeenCalledTimes(1);
    expect(seatA().tokens[2]).toBe(before.tokens[2] - 1);
    expect(seatA().actionCards).toBe(before.actionCards + 2);
    game.transport.close();
  },
  120_000,
);

it.skipIf(!built)(
  "shows the draft of a secondary that another tab of the seat changed, and does not write it back",
  async () => {
    const game = await open();
    const { flow, mine, menu, stepUntil, drafts, emit } = game;
    await stepUntil(() => flow().heading === "Politics" && !!mine()?.open);
    const option = menu(mine()?.open)[0].intent;
    const update = game.last()!;
    const { primary, card } = update.secondary!;
    const draft = {
      round: update.view.round,
      primary,
      card,
      answers: [{ id: option.type === "chooseOption" ? option.option : "", label: "decline" }],
      ready: true,
    };
    vi.mocked(drafts.set).mockClear();
    await emit("secondary", draft);
    await waitFor(() =>
      expect(mine()).toMatchObject({ status: { sign: "✎", label: "Draft ready" } }),
    );
    expect(mine()?.text).toMatch(/^Decline · not sent/);
    expect(drafts.set).not.toHaveBeenCalled();
    // The other tab starts over: the row is the draft again, with nothing answered.
    await emit("secondary", null);
    await waitFor(() => expect(mine()?.status.label).toBe("Private draft"));
    expect(menu(mine()?.open)).toHaveLength(2);
    expect(drafts.set).not.toHaveBeenCalled();
    game.transport.close();
  },
  120_000,
);

it.skipIf(!built)(
  "online the server sends a ready draft: the page sends none, and says when the server made it not ready",
  async () => {
    const game = await open({ server: true });
    const { flow, dispatch, button, mine, menu, stepUntil, emit } = game;
    await stepUntil(() => flow().heading === "Politics" && !!mine()?.open);
    await dispatch(menu(mine()?.open)[0].intent);
    await dispatch({ type: "flow", action: "resolve" });
    await waitFor(() => expect(button("Mark draft ready")?.disabled).toBeFalsy());
    await dispatch({ type: "flow", action: "ready" });
    expect(mine()?.status.label).toBe("Draft ready");
    const ready = game.drafts.get("secondary") as { ready: boolean };
    expect(ready.ready).toBe(true);

    // The game comes to the seat. This game sends nothing for it, so it asks: the page shows
    // the question and sends no draft.
    await stepUntil(() => flow().pill?.label === "Your decision");
    expect(game.applyDraft).not.toHaveBeenCalled();

    // The server found that the draft no longer fits: every tab says so.
    await emit("secondary", { ...ready, ready: false }, true);
    await waitFor(() =>
      expect(flow().footer.note).toBe("Your draft no longer fits the game. It is not ready."),
    );
    expect(mine()?.status.label).not.toBe("Draft ready");
    expect(game.applyDraft).not.toHaveBeenCalled();
    game.transport.close();
  },
  120_000,
);
