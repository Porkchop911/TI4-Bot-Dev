import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { renderHook, act } from "@testing-library/react";
import type { HistoryStatus, PendingChoiceDto } from "../protocol/types.ts";
import { resolveStep, type SecondaryPlan, type StepResolution } from "../presentation/secondaryPlan.ts";
import { AUTO_PLAY_DELAY_MS, AUTO_PLAY_HOLD_LIMIT_MS, useSecondaryAutoPlay } from "./useSecondaryAutoPlay.ts";
import { SECONDARY_PREP_MODE_KEY } from "./useSecondaryPrepMode.ts";
import { option, secondaryChoice, stepChoice } from "../test/secondaryPrepFixtures.ts";

const hist = (cursor: number, redo_count = 0, generation = 0): HistoryStatus => ({ cursor, redo_count, generation });
const plan: SecondaryPlan = { card: "pok7technology", follow: true, tech: "amd" };

interface Props {
  choice: PendingChoiceDto | null;
  history?: HistoryStatus;
  viewerSeat?: string | null;
  busy?: boolean;
  plan?: SecondaryPlan | null;
  logExtended?: boolean;
}

function setup(initial: Props) {
  const submitOption = vi.fn().mockResolvedValue(undefined);
  const submitTokens = vi.fn().mockResolvedValue(undefined);
  const hook = renderHook(
    (p: Props) => {
      const viewerSeat = p.viewerSeat === undefined ? "b" : p.viewerSeat;
      const resolution: StepResolution = resolveStep(p.plan === undefined ? plan : p.plan, p.choice, viewerSeat);
      return useSecondaryAutoPlay({
        choice: p.choice,
        viewerSeat,
        history: p.history,
        logExtended: p.logExtended,
        busy: p.busy,
        resolution,
        submitOption,
        submitTokens,
      });
    },
    { initialProps: initial },
  );
  const wait = () =>
    act(() => {
      vi.advanceTimersByTime(AUTO_PLAY_DELAY_MS + 10);
    });
  return { submitOption, submitTokens, hook, wait };
}

const windowChoice = (nonce = "w1") => secondaryChoice("pok7technology", { nonce });
const techChoice = (nonce = "t1") =>
  stepChoice("research_technology", [option("amd", "research"), option("decline", "decline")], nonce);

describe("useSecondaryAutoPlay", () => {
  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem(SECONDARY_PREP_MODE_KEY, "auto");
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  it("sends the prepared answer exactly once, after a visible delay, when the game moved forward to it", () => {
    const { submitOption, hook, wait } = setup({ choice: null, history: hist(5) });
    hook.rerender({ choice: windowChoice(), history: hist(6) });
    expect(hook.result.current.pending?.text).toMatch(/Follow Technology/);
    expect(submitOption).not.toHaveBeenCalled();
    wait();
    expect(submitOption).toHaveBeenCalledTimes(1);
    expect(submitOption).toHaveBeenCalledWith("yes");
    expect(hook.result.current.pending).toBeNull();
    // Re-rendering the same decision never sends it again.
    hook.rerender({ choice: windowChoice(), history: hist(6) });
    wait();
    expect(submitOption).toHaveBeenCalledTimes(1);
  });

  it("holds the decision UI back from the first render of the decision until cancelled or replaced", () => {
    const { hook, wait } = setup({ choice: null, history: hist(5) });
    expect(hook.result.current.holding).toBe(false);
    hook.rerender({ choice: windowChoice(), history: hist(6) });
    expect(hook.result.current.holding).toBe(true);
    act(() => hook.result.current.cancel());
    expect(hook.result.current.holding).toBe(false);
    // An unarmed decision (no forward progress) is never held.
    hook.rerender({ choice: windowChoice("w2"), history: hist(6) });
    wait();
    expect(hook.result.current.holding).toBe(false);
  });

  it("answers the follow-up steps of the same window one by one", () => {
    const { submitOption, hook, wait } = setup({ choice: null, history: hist(5) });
    hook.rerender({ choice: windowChoice(), history: hist(6) });
    wait();
    hook.rerender({ choice: techChoice(), history: hist(7) });
    wait();
    expect(submitOption.mock.calls).toEqual([["yes"], ["amd"]]);
  });

  it("can be cancelled during the delay and then sends nothing", () => {
    const { submitOption, hook, wait } = setup({ choice: null, history: hist(5) });
    hook.rerender({ choice: windowChoice(), history: hist(6) });
    act(() => hook.result.current.cancel());
    expect(hook.result.current.pending).toBeNull();
    wait();
    expect(submitOption).not.toHaveBeenCalled();
  });

  it("does nothing in the default Review mode, or when mode is switched back during the delay", () => {
    localStorage.removeItem(SECONDARY_PREP_MODE_KEY);
    const review = setup({ choice: null, history: hist(5) });
    review.hook.rerender({ choice: windowChoice(), history: hist(6) });
    review.wait();
    expect(review.submitOption).not.toHaveBeenCalled();
    expect(review.hook.result.current.pending).toBeNull();

    localStorage.setItem(SECONDARY_PREP_MODE_KEY, "auto");
    const flipped = setup({ choice: null, history: hist(5) });
    flipped.hook.rerender({ choice: windowChoice(), history: hist(6) });
    localStorage.setItem(SECONDARY_PREP_MODE_KEY, "review");
    flipped.wait();
    expect(flipped.submitOption).not.toHaveBeenCalled();
  });

  it("never sends a prepared answer that no longer validates (Needs review)", () => {
    const { submitOption, hook, wait } = setup({ choice: null, history: hist(5) });
    hook.rerender({ choice: secondaryChoice("pok7technology", { nonce: "w2" }, [option("no")]), history: hist(6) });
    wait();
    expect(submitOption).not.toHaveBeenCalled();
    hook.rerender({ choice: stepChoice("research_technology", [option("nm", "research")], "t2"), history: hist(7) });
    wait();
    expect(submitOption).not.toHaveBeenCalled();
  });

  it("sends a Leadership plan as one token batch", () => {
    const leadership: SecondaryPlan = { card: "pok1leadership", follow: true, leadership: { pools: { tactic: 1, fleet: 0, strategic: 0 } } };
    const choice = secondaryChoice("pok1leadership", {
      nonce: "l1",
      details: {
        kind: "strategy_secondary",
        card: "pok1leadership",
        costs_token: false,
        mode: "buy",
        pools: { tactic: 3, fleet: 3, strategic: 2 },
        reinforcements: 10,
        purchase: { cost: 3, influence_available: 3, max: 1, trade_goods: 0, trade_good_worth: 1, planets: [{ id: "jord", worth: 3 }] },
      },
    });
    const { submitOption, submitTokens, hook, wait } = setup({ choice: null, history: hist(5), plan: leadership });
    hook.rerender({ choice, history: hist(6), plan: leadership });
    wait();
    expect(submitOption).not.toHaveBeenCalled();
    expect(submitTokens).toHaveBeenCalledTimes(1);
    expect(submitTokens.mock.calls[0][0][0]).toEqual({ kind: "purchase", buy: true });
  });

  it("still fires when the same decision becomes actionable a moment after it opened", () => {
    const { submitOption, hook, wait } = setup({ choice: null, history: hist(5), plan: null });
    // The game moved forward to the decision, but the plan has nothing to say yet.
    hook.rerender({ choice: windowChoice(), history: hist(6), plan: null });
    wait();
    expect(submitOption).not.toHaveBeenCalled();
    // The same nonce, now with a resolvable plan: the armed state was kept for it.
    hook.rerender({ choice: windowChoice(), history: hist(6), plan });
    wait();
    expect(submitOption).toHaveBeenCalledTimes(1);
  });

  it("does not carry an unused armed state over to the next decision", () => {
    const { submitOption, hook, wait } = setup({ choice: null, history: hist(5), plan: null });
    hook.rerender({ choice: windowChoice("w1"), history: hist(6), plan: null });
    hook.rerender({ choice: windowChoice("w2"), history: hist(6), plan });
    wait();
    expect(submitOption).not.toHaveBeenCalled();
  });

  describe("guards", () => {
    it("never acts for a spectator or for another seat's decision", () => {
      const spectator = setup({ choice: null, history: hist(5), viewerSeat: null });
      spectator.hook.rerender({ choice: windowChoice(), history: hist(6), viewerSeat: null });
      spectator.wait();
      expect(spectator.submitOption).not.toHaveBeenCalled();

      const other = setup({ choice: null, history: hist(5), viewerSeat: "c" });
      other.hook.rerender({ choice: windowChoice(), history: hist(6), viewerSeat: "c" });
      other.wait();
      expect(other.submitOption).not.toHaveBeenCalled();
    });

    it("does nothing without a prepared plan", () => {
      const { submitOption, hook, wait } = setup({ choice: null, history: hist(5), plan: null });
      hook.rerender({ choice: windowChoice(), history: hist(6), plan: null });
      wait();
      expect(submitOption).not.toHaveBeenCalled();
    });

    it("does not send on a page load or reconnect that shows the decision straight away", () => {
      const { submitOption, hook, wait } = setup({ choice: windowChoice(), history: hist(6) });
      wait();
      expect(submitOption).not.toHaveBeenCalled();
      hook.rerender({ choice: windowChoice(), history: hist(6) });
      wait();
      expect(submitOption).not.toHaveBeenCalled();
    });

    it("does not send after an undo, a redo or a restore", () => {
      const undo = setup({ choice: null, history: hist(8) });
      undo.hook.rerender({ choice: null, history: hist(6, 2, 1) });
      undo.hook.rerender({ choice: windowChoice("u1"), history: hist(6, 2, 1) });
      undo.wait();
      expect(undo.submitOption).not.toHaveBeenCalled();

      const redo = setup({ choice: null, history: hist(6, 2, 1) });
      redo.hook.rerender({ choice: windowChoice("r1"), history: hist(7, 1, 1) });
      redo.wait();
      expect(redo.submitOption).not.toHaveBeenCalled();

      const restore = setup({ choice: null, history: hist(6, 0, 0) });
      restore.hook.rerender({ choice: windowChoice("s1"), history: hist(9, 0, 1) });
      restore.wait();
      expect(restore.submitOption).not.toHaveBeenCalled();
    });

    it("still sends after a committed batch: generation bumped, log only grew, cursor moved forward", () => {
      const batch = setup({ choice: null, history: hist(6, 0, 0), logExtended: true });
      batch.hook.rerender({ choice: windowChoice("b1"), history: hist(9, 0, 1), logExtended: true });
      batch.wait();
      expect(batch.submitOption).toHaveBeenCalledWith("yes");

      // The same history change with a rewritten log (restore) stays a manual question.
      const rewritten = setup({ choice: null, history: hist(6, 0, 0), logExtended: false });
      rewritten.hook.rerender({ choice: windowChoice("b2"), history: hist(9, 0, 1), logExtended: false });
      rewritten.wait();
      expect(rewritten.submitOption).not.toHaveBeenCalled();
    });

    it("does not send while a pipeline or history change is busy", () => {
      const { submitOption, hook, wait } = setup({ choice: null, history: hist(5), busy: true });
      hook.rerender({ choice: windowChoice(), history: hist(6), busy: true });
      wait();
      expect(submitOption).not.toHaveBeenCalled();
    });

    it("lets only one tab send (second-tab claim)", () => {
      const first = setup({ choice: null, history: hist(5) });
      const second = setup({ choice: null, history: hist(5) });
      first.hook.rerender({ choice: windowChoice("same"), history: hist(6) });
      second.hook.rerender({ choice: windowChoice("same"), history: hist(6) });
      first.wait();
      second.wait();
      expect(first.submitOption.mock.calls.length + second.submitOption.mock.calls.length).toBe(1);
    });

    it("keeps the decision UI held while the answer is still being sent, however long that takes", async () => {
      const { submitOption, hook, wait } = setup({ choice: null, history: hist(5) });
      let settle!: () => void;
      submitOption.mockReturnValue(new Promise<void>((done) => (settle = done)));
      hook.rerender({ choice: windowChoice("slow"), history: hist(6) });
      wait();
      expect(submitOption).toHaveBeenCalledTimes(1);
      // A slow batch commit outlasts the safety limit: the open decision is still the old version.
      act(() => {
        vi.advanceTimersByTime(AUTO_PLAY_HOLD_LIMIT_MS * 3);
      });
      expect(hook.result.current.holding).toBe(true);
      await act(async () => settle());
      expect(hook.result.current.holding).toBe(true);
      act(() => {
        vi.advanceTimersByTime(AUTO_PLAY_HOLD_LIMIT_MS + 10);
      });
      expect(hook.result.current.holding).toBe(false);
    });

    it("does not retry a failed send (the player answers by hand)", () => {
      const { submitOption, hook, wait } = setup({ choice: null, history: hist(5) });
      submitOption.mockRejectedValue(new Error("stale"));
      hook.rerender({ choice: windowChoice("f1"), history: hist(6) });
      wait();
      hook.rerender({ choice: windowChoice("f1"), history: hist(6) });
      wait();
      expect(submitOption).toHaveBeenCalledTimes(1);
    });
  });
});
