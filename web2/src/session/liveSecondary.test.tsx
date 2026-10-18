import { existsSync, readFileSync } from "node:fs";
import { resolve } from "node:path";
import { act, renderHook, waitFor } from "@testing-library/react";
import { expect, it, vi } from "vitest";
import type { BlockView, FlowActionView, Intent, SeatRowView } from "../model";
import type { SeatStore, StagedSlot } from "./seatStore";
import { type LiveSession, useLiveSession } from "./useLiveSession";
import { type Engine, type HostSave, createWasmTransport } from "./wasmTransport";
import type { SessionUpdate } from "./wire";

// The private draft of a secondary, on the real engine (`scripts/build-wasm.sh`): a game of
// seed 4 with seat a on this side. The other seats wait for a step before each decision, so
// the page is shown the game while another seat resolves its primary. A seat plays Leadership
// first, which asks seat a nothing; then another seat plays Politics.
// In the DOM project of the tests a module has no file address: the tests run from `web2/`.
const WASM = resolve(process.cwd(), "src/session/ti4.wasm");
const built = existsSync(WASM);

/** The slots of seat a in a save of the engine (`HostSession::save_state`). */
type SavedSlots = Record<string, { slots: Partial<Record<StagedSlot, { value?: unknown }>> }>;

/**
 * `memory`: the page keeps what it stages in a store of the test, which another tab of the seat
 * changes (`emit`); the engine does not see it. Else the store is the engine's
 * (`Transport.staged`), which keeps it in the saved game and sends a ready draft itself.
 */
async function open({ memory = false } = {}) {
  const engine: Engine = {
    module: await WebAssembly.compile(readFileSync(WASM)),
    hash: "test",
    scenarios: [],
    catalog: null,
  };
  const saves: HostSave[] = [];
  const transport = createWasmTransport(
    engine,
    { seed: 4, players: 8, humans: 1 },
    { stepping: true, onSave: (save) => saves.push(save) },
  );
  /** What the last save of the engine keeps in a slot of seat a. */
  const kept = (slot: StagedSlot) =>
    (saves.at(-1)?.staged as SavedSlots | undefined)?.a?.slots[slot]?.value ?? null;
  // The store of a seat that another tab or the game changes, as the server tells it (`emit`).
  const values = new Map<StagedSlot, unknown>();
  const listeners = new Set<(slot: StagedSlot, value: unknown, server: boolean) => void>();
  const engineStore = transport.staged!;
  const drafts: SeatStore = memory
    ? {
        get: (slot) => values.get(slot) ?? null,
        set: vi.fn((slot: StagedSlot, value: unknown) => void values.set(slot, value ?? null)),
        subscribe(listener) {
          listeners.add(listener);
          return () => listeners.delete(listener);
        },
      }
    : { get: engineStore.get, set: vi.fn(engineStore.set), subscribe: engineStore.subscribe };
  const emit = (slot: StagedSlot, value: unknown, byServer = false) =>
    act(async () => {
      values.set(slot, value);
      for (const listener of listeners) {
        listener(slot, value, byServer);
      }
    });
  const applyDraft = vi.spyOn(transport, "applyDraft");
  const submitChoice = vi.spyOn(transport, "submitChoice");
  /** The strategic action of the last update of the game. */
  let last: SessionUpdate | null = null;
  /** The last draft that the page asked for, and the last one that the game answered. */
  let asked: unknown = null;
  let answered: unknown = null;
  const draft = transport.draft.bind(transport);
  transport.draft = (script, scope) => {
    asked = script;
    draft(script, scope);
  };
  transport.subscribe((event) => {
    if (event.kind === "update") {
      last = event.update;
    } else if (event.kind === "draft") {
      answered = event.script;
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
  /** The list of the seats, when the panel is a strategic action: the part of the Log. */
  const seats = (): SeatRowView[] | null => {
    const block = flow().blocks.find((item) => item.kind === "seats" && item.role === "inform");
    return block?.kind === "seats" ? block.rows : null;
  };
  /** The row of the viewer: with its editor, the decision of the step sheet, when it has one. */
  const mine = () => {
    const sheet = flow().blocks.find((item) => item.kind === "seats" && item.role === "decide");
    return (
      (sheet?.kind === "seats" ? sheet.rows[0] : null) ?? seats()?.find((row) => row.you) ?? null
    );
  };
  const menu = (blocks: BlockView[] | undefined) =>
    (blocks ?? []).flatMap((block) => (block.kind === "menu" ? block.rows : []));
  /** Answers the open choice of the game with the option of this id. */
  const answer = async (option: string) => {
    await dispatch({ type: "chooseOption", option });
    await dispatch({ type: "flow", action: "resolve" });
  };
  /**
   * Lets the other seats decide, one step at a time, until `reached`. The engine answers in its
   * Worker: each step waits for the update after it, and for the answer to the draft of the page.
   */
  const stepUntil = async (reached: () => boolean) => {
    for (let steps = 0; steps < 400; steps += 1) {
      await waitFor(() => expect(answered === asked && (reached() || !!button("Step"))).toBe(true));
      if (reached()) {
        return;
      }
      const before = last;
      await dispatch({ type: "simulate" });
      await waitFor(() => expect(last).not.toBe(before));
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
    // The list of the Log has nothing open; the row of the viewer with its draft is the sheet's.
    expect(rows.filter((row) => row.open)).toHaveLength(0);
    expect(rows.find((row) => row.you)?.status.label).toBe("Private draft");
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
    await waitFor(() => {
      // The button is there once the game answered the draft.
      expect(button("Mark draft ready")).toBeDefined();
      expect(button("Mark draft ready")?.disabled).toBeFalsy();
    });
    expect(mine()).toMatchObject({ status: { label: "Private draft" }, text: "Decline" });
    expect(game.submitChoice).toHaveBeenCalledTimes(1);

    // It starts over with nothing answered.
    await dispatch({ type: "startOver" });
    await waitFor(() => expect(menu(mine()?.open)).toHaveLength(2));
    await dispatch(options[0].intent);
    await dispatch({ type: "flow", action: "resolve" });
    await waitFor(() => {
      // The button is there once the game answered the draft.
      expect(button("Mark draft ready")).toBeDefined();
      expect(button("Mark draft ready")?.disabled).toBeFalsy();
    });

    // A ready draft is closed and says what it will do. The engine keeps it in the saved game.
    await dispatch({ type: "flow", action: "ready" });
    await waitFor(() =>
      expect(mine()).toMatchObject({ status: { sign: "✎", label: "Draft ready" } }),
    );
    expect(mine()?.text).toMatch(/^Decline · not sent, resolves after /);
    expect(mine()?.open).toBeUndefined();
    await dispatch({ type: "flow", action: "change" });
    expect(mine()?.status.label).toBe("Private draft");
    await dispatch({ type: "flow", action: "ready" });
    await waitFor(() =>
      expect(kept("secondary")).toMatchObject({
        card: "pok3politics",
        ready: true,
        answers: [{ label: "decline" }],
      }),
    );

    // The game comes to the seat: the engine sends the draft, and the seat is not asked.
    const asked: string[] = [];
    await stepUntil(() => {
      asked.push(flow().pill?.label ?? "");
      return mine()?.status.label === "Passed" || game.seats() === null;
    });
    expect(asked).not.toContain("Your decision");
    expect(game.applyDraft).not.toHaveBeenCalled();
    expect(game.submitChoice).toHaveBeenCalledTimes(1);
    await waitFor(() => expect(game.drafts.get("secondary")).toBeNull());
    expect(kept("secondary")).toBeNull();
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
    await waitFor(() => {
      // The button is there once the game answered the draft.
      expect(button("Mark draft ready")).toBeDefined();
      expect(button("Mark draft ready")?.disabled).toBeFalsy();
    });
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
    const game = await open({ memory: true });
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
    await waitFor(() => expect(menu(mine()?.open)).toHaveLength(2));
    expect(drafts.set).not.toHaveBeenCalled();
    game.transport.close();
  },
  120_000,
);

it.skipIf(!built)(
  "the page sends no ready draft, and says when the game made it not ready",
  async () => {
    const game = await open({ memory: true });
    const { flow, dispatch, button, mine, menu, stepUntil, emit } = game;
    await stepUntil(() => flow().heading === "Politics" && !!mine()?.open);
    await dispatch(menu(mine()?.open)[0].intent);
    await dispatch({ type: "flow", action: "resolve" });
    await waitFor(() => {
      // The button is there once the game answered the draft.
      expect(button("Mark draft ready")).toBeDefined();
      expect(button("Mark draft ready")?.disabled).toBeFalsy();
    });
    await dispatch({ type: "flow", action: "ready" });
    expect(mine()?.status.label).toBe("Draft ready");
    const ready = game.drafts.get("secondary") as { ready: boolean };
    expect(ready.ready).toBe(true);

    // The game comes to the seat. The engine does not see this store and sends nothing, so it
    // asks: the page shows the question and sends no draft.
    await stepUntil(() => flow().pill?.label === "Your decision");
    expect(game.applyDraft).not.toHaveBeenCalled();

    // The game found that the draft no longer fits: every tab says so.
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
