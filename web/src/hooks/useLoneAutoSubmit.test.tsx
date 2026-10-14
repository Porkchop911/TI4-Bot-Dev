import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import type { ChoiceOptionDto, HistoryStatus, PendingChoiceDto } from "../protocol/types.ts";
import { loneAction } from "../presentation/loneChoice.ts";
import { LONE_SUBMIT_DELAY_MS, useLoneAutoSubmit } from "./useLoneAutoSubmit.ts";
import { AUTO_SUBMIT_KEY } from "./useAutoSubmitSetting.ts";

const opt = (id: string, kind = "action"): ChoiceOptionDto => ({ id, kind, label: id });

const menu = (nonce: string, options: ChoiceOptionDto[], closing = false): PendingChoiceDto => ({
  actor: "a",
  nonce,
  prompt: "action phase",
  options,
  details: { kind: "turn_menu", closing },
});

const activation = (nonce: string, ids: string[]): PendingChoiceDto => ({
  actor: "a",
  nonce,
  prompt: "activate a system",
  context: { subtype: "activate_system" } as PendingChoiceDto["context"],
  options: ids.map((id) => opt(id, "activate")),
});

const hist = (cursor: number, redo_count = 0, generation = 0): HistoryStatus => ({
  cursor,
  redo_count,
  generation,
});

interface Props {
  choice: PendingChoiceDto | null;
  history?: HistoryStatus;
  viewerSeat?: string | null;
  busy?: boolean;
}

function setup(initial: Props) {
  const submit = vi.fn().mockResolvedValue(undefined);
  const onNotice = vi.fn();
  const hook = renderHook(
    (p: Props) =>
      useLoneAutoSubmit({
        viewerSeat: "a",
        ...p,
        submit,
        onNotice,
      }),
    { initialProps: initial },
  );
  const wait = () =>
    act(() => {
      vi.advanceTimersByTime(LONE_SUBMIT_DELAY_MS + 10);
    });
  return { submit, onNotice, hook, wait };
}

describe("loneAction", () => {
  it("matches only the bare strategic option on the opening menu", () => {
    expect(loneAction(menu("1", [opt("strategic")]))?.kind).toBe("strategic");
    expect(loneAction(menu("1", [opt("strategic"), opt("pass")]))).toBeNull();
    expect(loneAction(menu("1", [opt("strategic"), opt("tactical")]))).toBeNull();
    expect(loneAction(menu("1", [opt("strategic|x")]))).toBeNull();
    expect(loneAction(menu("1", [opt("pass")]))).toBeNull();
    expect(loneAction(menu("1", [opt("strategic")], true))).toBeNull();
  });
  it("matches a single activation only", () => {
    expect(loneAction(activation("1", ["33"]))).toMatchObject({ kind: "activate", optionId: "33" });
    expect(loneAction(activation("1", ["33", "34"]))).toBeNull();
    const free = activation("1", ["33"]);
    free.prompt = "Warfare: a tactical action without a command token";
    free.context = { subtype: "te6warfare" } as PendingChoiceDto["context"];
    expect(loneAction(free)).toBeNull();
  });
});

const movement = (nonce: string, ids: string[]): PendingChoiceDto => ({
  actor: "a",
  nonce,
  prompt: "movement",
  options: ids.map((id) => opt(id, id === "done_moving" ? "decline" : "move")),
});

describe("loneAction movement", () => {
  it("matches movement only when finishing is the one option", () => {
    expect(loneAction(movement("1", ["done_moving"]))).toMatchObject({ kind: "movement", optionId: "done_moving" });
    expect(loneAction(movement("1", ["move|1|0", "done_moving"]))).toBeNull();
  });
});

describe("useLoneAutoSubmit", () => {
  beforeEach(() => {
    localStorage.clear();
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  it("submits a lone strategic action after the game moved forward, with a notice", () => {
    const { submit, onNotice, hook, wait } = setup({ choice: null, history: hist(5) });
    hook.rerender({ choice: menu("n1", [opt("strategic")]), history: hist(6) });
    wait();
    expect(submit).toHaveBeenCalledTimes(1);
    expect(submit).toHaveBeenCalledWith("strategic");
    expect(onNotice).toHaveBeenCalledWith({ id: "lone-n1", text: "take your strategic action" });
  });

  it("still asks when pass or tactical is also offered", () => {
    for (const extra of ["pass", "tactical"]) {
      localStorage.clear();
      const { submit, hook, wait } = setup({ choice: null, history: hist(5) });
      hook.rerender({ choice: menu("n1", [opt("strategic"), opt(extra)]), history: hist(6) });
      wait();
      expect(submit).not.toHaveBeenCalled();
    }
  });

  it("submits a lone activation and says which system", () => {
    const { submit, onNotice, hook, wait } = setup({ choice: null, history: hist(5) });
    hook.rerender({ choice: activation("n2", ["33"]), history: hist(6) });
    wait();
    expect(submit).toHaveBeenCalledTimes(1);
    expect(submit).toHaveBeenCalledWith("33");
    expect(onNotice).toHaveBeenCalledWith({ id: "lone-n2", text: "activate system 33" });
  });

  it("finishes a movement with no movable unit and keeps its dialog closed meanwhile", () => {
    const { submit, onNotice, hook, wait } = setup({ choice: null, history: hist(5) });
    hook.rerender({ choice: movement("m1", ["done_moving"]), history: hist(6) });
    expect(hook.result.current.answeringNonce).toBe("m1");
    wait();
    expect(submit).toHaveBeenCalledWith("done_moving");
    expect(onNotice).toHaveBeenCalledWith({ id: "lone-m1", text: "no unit can move, finishing movement" });
  });

  it("asks (dialog stays) for movement with moves on offer, or when the setting is off", () => {
    const a = setup({ choice: null, history: hist(5) });
    a.hook.rerender({ choice: movement("m1", ["move|1|0", "done_moving"]), history: hist(6) });
    a.wait();
    expect(a.submit).not.toHaveBeenCalled();
    expect(a.hook.result.current.answeringNonce).toBeNull();
    localStorage.setItem(AUTO_SUBMIT_KEY, "false");
    const b = setup({ choice: null, history: hist(5) });
    b.hook.rerender({ choice: movement("m2", ["done_moving"]), history: hist(6) });
    b.wait();
    expect(b.submit).not.toHaveBeenCalled();
    expect(b.hook.result.current.answeringNonce).toBeNull();
  });

  it("asks when several systems can be activated", () => {
    const { submit, hook, wait } = setup({ choice: null, history: hist(5) });
    hook.rerender({ choice: activation("n2", ["33", "34"]), history: hist(6) });
    wait();
    expect(submit).not.toHaveBeenCalled();
  });

  it("never acts for a spectator or for another seat's decision", () => {
    const a = setup({ choice: null, history: hist(5), viewerSeat: null });
    a.hook.rerender({ choice: menu("n1", [opt("strategic")]), history: hist(6), viewerSeat: null });
    a.wait();
    expect(a.submit).not.toHaveBeenCalled();
    const b = setup({ choice: null, history: hist(5), viewerSeat: "b" });
    b.hook.rerender({ choice: menu("n1", [opt("strategic")]), history: hist(6), viewerSeat: "b" });
    b.wait();
    expect(b.submit).not.toHaveBeenCalled();
  });

  it("submits once across re-renders of the same decision", () => {
    const { submit, hook, wait } = setup({ choice: null, history: hist(5) });
    const choice = menu("n1", [opt("strategic")]);
    hook.rerender({ choice, history: hist(6) });
    hook.rerender({ choice: { ...choice }, history: { ...hist(6) } });
    wait();
    hook.rerender({ choice: { ...choice }, history: hist(6) });
    wait();
    expect(submit).toHaveBeenCalledTimes(1);
  });

  it("does not act on a decision already open at load or after a reconnect with no progress", () => {
    const choice = menu("n1", [opt("strategic")]);
    const { submit, hook, wait } = setup({ choice, history: hist(6) });
    wait();
    hook.rerender({ choice: { ...choice }, history: hist(6) }); // reconnect snapshot
    wait();
    expect(submit).not.toHaveBeenCalled();
  });

  it("does not act right after an undo or a redo, but acts again once play moves forward", () => {
    const { submit, hook, wait } = setup({
      choice: menu("n0", [opt("pass"), opt("strategic")]),
      history: hist(6),
    });
    hook.rerender({ choice: menu("n1", [opt("strategic")]), history: hist(5, 1) }); // undo
    wait();
    expect(submit).not.toHaveBeenCalled();
    // redo back to the tip lands on a lone decision: still not answered
    hook.rerender({ choice: menu("n2", [opt("strategic")]), history: hist(6, 0) });
    wait();
    expect(submit).not.toHaveBeenCalled();
    // after a manual answer the game moves forward and the next lone decision is taken
    hook.rerender({ choice: menu("n3", [opt("strategic")]), history: hist(7, 0) });
    wait();
    expect(submit).toHaveBeenCalledTimes(1);
  });

  it("does not act after a restore (generation change)", () => {
    const { submit, hook, wait } = setup({ choice: null, history: hist(5, 0, 0) });
    hook.rerender({ choice: menu("n1", [opt("strategic")]), history: hist(9, 0, 1) });
    wait();
    expect(submit).not.toHaveBeenCalled();
  });

  it("an undo of the auto-submitted action is not answered again", () => {
    const { submit, hook, wait } = setup({ choice: null, history: hist(5) });
    hook.rerender({ choice: menu("n1", [opt("strategic")]), history: hist(6) });
    wait();
    expect(submit).toHaveBeenCalledTimes(1);
    hook.rerender({ choice: null, history: hist(7) });
    hook.rerender({ choice: menu("n1b", [opt("strategic")]), history: hist(6, 1) });
    wait();
    expect(submit).toHaveBeenCalledTimes(1);
  });

  it("does nothing when the setting is off", () => {
    localStorage.setItem(AUTO_SUBMIT_KEY, "false");
    const { submit, hook, wait } = setup({ choice: null, history: hist(5) });
    hook.rerender({ choice: menu("n1", [opt("strategic")]), history: hist(6) });
    wait();
    expect(submit).not.toHaveBeenCalled();
  });

  it("leaves the decision alone while a pipeline or history change is busy", () => {
    const { submit, hook, wait } = setup({ choice: null, history: hist(5), busy: true });
    hook.rerender({ choice: menu("n1", [opt("strategic")]), history: hist(6), busy: true });
    wait();
    expect(submit).not.toHaveBeenCalled();
  });

  it("a second tab that already claimed the decision does not send again", () => {
    localStorage.setItem("ti4_lone_submit_claim", "n1");
    const { submit, hook, wait } = setup({ choice: null, history: hist(5) });
    hook.rerender({ choice: menu("n1", [opt("strategic")]), history: hist(6) });
    wait();
    expect(submit).not.toHaveBeenCalled();
  });

  it("a rejected submit leaves the decision for a click and is not retried", async () => {
    const { submit, hook, wait } = setup({ choice: null, history: hist(5) });
    submit.mockRejectedValue(new Error("stale"));
    const choice = menu("n1", [opt("strategic")]);
    hook.rerender({ choice, history: hist(6) });
    wait();
    hook.rerender({ choice: { ...choice }, history: hist(7) });
    wait();
    expect(submit).toHaveBeenCalledTimes(1);
  });
});
