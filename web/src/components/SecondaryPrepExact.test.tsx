import React, { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import type {
  GameEvent,
  HistoryStatus,
  PendingChoiceDto,
  PlayerView,
} from "../protocol/types.ts";
import {
  PreviewUnsupportedError,
  type BasketPlan,
  type SecondaryPreviewReply,
} from "../protocol/client.ts";
import { ChoiceRendererDispatcher } from "./GameShell.tsx";
import { SecondaryPrepHost } from "./SecondaryPrepHost.tsx";
import {
  PREVIEW_REFRESH_DEBOUNCE_MS,
  PREVIEW_RETRY_MS,
  useSecondaryPrepare,
} from "../hooks/useSecondaryPrepare.ts";
import { PreparedHintProvider } from "../presentation/PreparedHint.tsx";
import { pendingFromEngine } from "../presentation/secondaryPreview.ts";
import { SECONDARY_PREP_MODE_KEY } from "../hooks/useSecondaryPrepMode.ts";
import { AUTO_PLAY_DELAY_MS } from "../hooks/useSecondaryAutoPlay.ts";
import { loadPlan } from "../hooks/useSecondaryPlan.ts";
import {
  boardWith,
  enginePayment,
  engineProduction,
  engineResearch,
  engineWindow,
  playedLog,
  player,
  previewReply,
  secondaryChoice,
  stepChoice,
} from "../test/secondaryPrepFixtures.ts";

const hist = (cursor: number, generation = 0): HistoryStatus => ({ cursor, redo_count: 0, generation });

type Preview = (card: string, primary: string, answers: readonly string[]) => Promise<SecondaryPreviewReply>;

interface HarnessProps {
  card: string;
  events?: GameEvent[];
  players?: PlayerView[];
  choice?: PendingChoiceDto | null;
  history?: HistoryStatus;
  refreshKey?: string;
  preview?: Preview;
  submit: (optionId: string) => Promise<void>;
  batch: (plan: BasketPlan) => Promise<void>;
  production?: (units: string[], destination: string, real: PendingChoiceDto) => Promise<void>;
}

/** The hook, the real decision renderer and the prepare chrome wired the way App.tsx wires them. */
const Harness: React.FC<HarnessProps> = (p) => {
  const list = p.players ?? [
    player("a", { strategy_cards: [p.card] }),
    player("b", { trade_goods: 4 }),
    player("c"),
  ];
  const board = boardWith([{ system: "18", planet: "jord", owner: "b" }]);
  const prep = useSecondaryPrepare({
    gameId: "g1",
    viewerSeat: "b",
    players: list,
    events: p.events!,
    board,
    phase: "action",
    history: p.history ?? hist(5),
    realChoice: p.choice ?? null,
    submitChoice: p.submit,
    submitBatch: p.batch,
    previewSecondary: p.preview,
    refreshKey: p.refreshKey,
    submitProduction: p.production,
  });
  const [minimized, setMinimized] = useState(false);
  const [selected, setSelected] = useState<string | undefined>();
  const record = Object.fromEntries(list.map((entry) => [entry.id, entry]));
  return (
    <PreparedHintProvider
      value={
        !prep.preparing && prep.realChoice
          ? prep.resolution.kind === "option"
            ? { optionId: prep.resolution.optionId, text: prep.resolution.text }
            : prep.resolution.kind === "production"
              ? { optionId: "", text: prep.resolution.text, builds: prep.resolution.units }
              : null
          : null
      }
    >
      <ChoiceRendererDispatcher
        choice={prep.shownChoice}
        viewerSeat="b"
        players={record}
        boardView={board}
        onSubmit={prep.preparing ? prep.prepareSubmit : p.submit}
        onSubmitBasketBatch={prep.preparing ? prep.prepareBatch : p.batch}
        isMinimized={minimized}
        onMinimizedChange={setMinimized}
        selectedOptionId={selected}
        onSelectOption={setSelected}
        selectedPlanetId={null}
        onSelectPlanet={() => {}}
      />
      <SecondaryPrepHost prep={prep} />
    </PreparedHintProvider>
  );
};

function setup(card: string, name: string, over: Partial<HarnessProps> = {}) {
  const submit = vi.fn().mockResolvedValue(undefined);
  const batch = vi.fn().mockResolvedValue(undefined);
  const production = vi.fn().mockResolvedValue(undefined);
  const props: HarnessProps = { card, events: playedLog(name), submit, batch, production, ...over };
  const view = render(<Harness {...props} />);
  const again = (next: Partial<HarnessProps>) => {
    Object.assign(props, next);
    view.rerender(<Harness {...props} />);
  };
  return { submit, batch, production, view, again };
}

const click = (id: string) => fireEvent.click(screen.getByTestId(id));
const open = async () => {
  await act(async () => {
    click("secondary-prep-chip");
  });
  await act(async () => {});
};
const plan = () => loadPlan("g1", "b")?.plan;
const settle = () => act(async () => {});

const WINDOWS: Record<string, [string, string]> = {
  pok7technology: ["Technology", "spend a strategy token and 4 resources to research"],
  pok6warfare: ["Warfare", "spend a strategy token to produce at home"],
};

/** A preview that answers like the engine: window, then the follow-ups by scripted answers. */
function engineLike(card: string, followUps: Record<string, SecondaryPreviewReply>): Preview & ReturnType<typeof vi.fn> {
  return vi.fn(async (_card: string, _primary: string, answers: readonly string[]) => {
    if (answers.length === 0) {
      return previewReply({
        status: "question",
        choice: engineWindow(card, WINDOWS[card][1]),
        step: 0,
      });
    }
    return followUps[answers.join(">")] ?? previewReply({ status: "complete" });
  }) as never;
}

beforeEach(() => {
  localStorage.clear();
  localStorage.removeItem(SECONDARY_PREP_MODE_KEY);
});
afterEach(() => vi.useRealTimers());

describe("preparing with the engine's exact options", () => {
  it("Technology: the engine's list (with a skip the estimate cannot know), its payment plan, no 'approximate' note", async () => {
    const preview = engineLike("pok7technology", {
      yes: previewReply({
        status: "question",
        choice: engineResearch(["amd", "ws"]),
        step: 1,
        payment: { cost: 4, planets: ["jord"], trade_goods: 1, worth: 5 },
      }),
    });
    const { submit, batch } = setup("pok7technology", "Technology", { preview });
    await open();
    // The window question is the engine's own, and says so.
    expect(preview).toHaveBeenCalledWith("pok7technology", "a", []);
    expect(screen.getByTestId("prepare-as-of-now").textContent).toMatch(/Exact options from the game as of now/);
    expect(screen.queryByTestId("prepare-approximate")).toBeNull();
    await act(async () => {
      click("secondary-yes-btn");
    });
    await settle();
    expect(preview).toHaveBeenCalledWith("pok7technology", "a", ["yes"]);
    expect(plan()).toEqual({ card: "pok7technology", follow: true });
    // The real list is the engine's two: the picker only lets you choose what the engine would
    // offer. War Sun is on it through a skip (the picker's own skip toggles apply as in the real
    // question); with nothing toggled only the prerequisite-free one is selectable.
    const card = (id: string) => document.querySelector(`[data-testid="tech-card-${id}"]`);
    expect(card("ws")).not.toBeNull();
    expect(card("amd")?.getAttribute("data-selectable")).toBe("true");
    expect(card("ws")?.getAttribute("data-selectable")).toBe("false");
    // A technology the engine does not offer is not selectable even when its prerequisites look met.
    expect(card("nm")?.getAttribute("data-selectable")).toBe("false");
    expect(screen.queryByTestId("prepare-approximate")).toBeNull();
    expect(screen.getByTestId("prepare-as-of-now")).toBeInTheDocument();
    expect(screen.getByTestId("prepare-payment-preview").textContent).toMatch(/Jord, 1 trade good/);
    fireEvent.click(card("amd") as HTMLElement);
    await act(async () => {
      click("confirm-research-btn");
    });
    await settle();
    expect(plan()).toEqual({ card: "pok7technology", follow: true, tech: "amd" });
    expect(submit).not.toHaveBeenCalled();
    expect(batch).not.toHaveBeenCalled();
  });

  it("falls back to the estimate, flagged approximate, when the server has no preview", async () => {
    const preview = vi.fn().mockRejectedValue(new PreviewUnsupportedError());
    setup("pok7technology", "Technology", { preview });
    await open();
    expect(screen.getByTestId("strategy-secondary-panel")).toBeInTheDocument();
    expect(screen.queryByTestId("prepare-as-of-now")).toBeNull();
    await act(async () => {
      click("secondary-yes-btn");
    });
    await settle();
    expect(screen.getByTestId("prepare-approximate").textContent).toMatch(/Approximate until the real question opens/);
    // Remembered: the unsupported server is not asked again for the next step.
    expect(preview).toHaveBeenCalledTimes(1);
  });

  it("falls back on a refusal or an unusable answer too, and never blocks preparing", async () => {
    const preview = vi.fn().mockResolvedValue({ kind: "refused", reason: "already_asked", detail: "asked" });
    setup("pok7technology", "Technology", { preview });
    await open();
    await act(async () => {
      click("secondary-yes-btn");
    });
    await settle();
    expect(screen.getByTestId("prepare-approximate")).toBeInTheDocument();
    expect(plan()).toEqual({ card: "pok7technology", follow: true });
  });

  it("says when the engine would not ask you as of now, and still lets you prepare", async () => {
    const preview = vi.fn().mockResolvedValue(
      previewReply({ status: "would_not_be_asked", blocker: "cannot_pay_resources" }),
    );
    setup("pok7technology", "Technology", { preview });
    await open();
    expect(screen.getByTestId("prepare-not-asked").textContent).toMatch(/cannot pay the 4 resources/);
    expect(screen.getByTestId("prepare-not-asked").textContent).toMatch(/applies if that changes/);
    await act(async () => {
      click("secondary-yes-btn");
    });
    await settle();
    expect(plan()).toEqual({ card: "pok7technology", follow: true });
  });

  it("asks again, after a pause, when the game moves while the panel is open", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    const preview = engineLike("pok7technology", {});
    const { again } = setup("pok7technology", "Technology", { preview, refreshKey: "k1" });
    await open();
    expect(preview).toHaveBeenCalledTimes(1);
    again({ refreshKey: "k2" });
    again({ refreshKey: "k3" });
    await act(async () => {
      await vi.advanceTimersByTimeAsync(PREVIEW_REFRESH_DEBOUNCE_MS - 50);
    });
    expect(preview).toHaveBeenCalledTimes(1);
    await act(async () => {
      await vi.advanceTimersByTimeAsync(100);
    });
    // One refresh for the burst, for the question being shown.
    expect(preview).toHaveBeenCalledTimes(2);
    expect(preview).toHaveBeenLastCalledWith("pok7technology", "a", []);
  });

  it("a 'too many previews' refusal is retried, not taken for 'no preview'", async () => {
    vi.useFakeTimers({ shouldAdvanceTime: true });
    let calls = 0;
    const preview = vi.fn(async (_card: string, _primary: string, answers: readonly string[]) => {
      calls += 1;
      if (calls === 1) return { kind: "refused", reason: "rate_limited", detail: "slow down" } as const;
      return answers.length === 0
        ? previewReply({ status: "question", choice: engineWindow("pok7technology", WINDOWS.pok7technology[1]), step: 0 })
        : previewReply({ status: "complete" });
    });
    setup("pok7technology", "Technology", { preview });
    await open();
    await act(async () => {
      await vi.advanceTimersByTimeAsync(PREVIEW_RETRY_MS + 50);
    });
    expect(preview).toHaveBeenCalledTimes(2);
    expect(screen.getByTestId("prepare-as-of-now")).toBeInTheDocument();
    expect(screen.queryByTestId("prepare-approximate")).toBeNull();
  });

  it("asks nothing before the panel is opened", async () => {
    const preview = engineLike("pok7technology", {});
    setup("pok7technology", "Technology", { preview });
    await settle();
    expect(preview).not.toHaveBeenCalled();
  });
});

describe("preparing a Warfare production with the real builder", () => {
  const build = (unit: string) => `build|${unit}|${unit === "infantry" ? 2 : 1}`;
  const previewFor = () =>
    engineLike("pok6warfare", {
      yes: previewReply({
        status: "question",
        choice: engineProduction([
          { unit: "infantry", cost: 1, count: 2 },
          { unit: "carrier", cost: 3, count: 1 },
        ]),
        step: 1,
      }),
      [`yes>${build("infantry")}`]: previewReply({
        status: "question",
        choice: enginePayment(1),
        step: 2,
      }),
    });

  const prepare = async () => {
    const ctx = setup("pok6warfare", "Warfare", { preview: previewFor() });
    await open();
    await act(async () => {
      click("secondary-yes-btn");
    });
    await settle();
    return ctx;
  };

  it("follow opens the real production builder on the engine's build list, saving builds records the plan", async () => {
    const { submit, batch, production } = await prepare();
    expect(screen.getByTestId("production-builder-drawer")).toBeInTheDocument();
    expect(screen.getByTestId(`produce-option-${build("infantry")}`)).toBeInTheDocument();
    expect(screen.getByTestId(`produce-option-${build("carrier")}`)).toBeInTheDocument();
    expect(screen.getByTestId("production-capacity-counter").textContent).toMatch(/0 \/ 3/);
    expect(screen.getByTestId("prepare-as-of-now")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId(`produce-unit-btn-${build("infantry")}`));
    fireEvent.click(screen.getByTestId(`produce-unit-btn-${build("carrier")}`));
    // Preparing: no build queue, the staged builds are saved as they are.
    fireEvent.click(screen.getByRole("button", { name: "Save these builds" }));
    await settle();
    expect(plan()).toEqual({
      card: "pok6warfare",
      follow: true,
      production: { builds: ["infantry", "carrier"] },
    });
    // The first build's payment is asked next, in the real payment drawer; the pick is stored by planet.
    expect(screen.getByTestId("payment-drawer")).toBeInTheDocument();
    fireEvent.click(screen.getByTestId("planet-card-exhaust|jord"));
    await act(async () => {
      click("confirm-payment-btn");
    });
    await settle();
    expect(plan()?.production).toEqual({
      builds: ["infantry", "carrier"],
      payment: { planets: ["jord"], tradeGoods: 0 },
    });
    expect(screen.queryByTestId("prepare-banner")).toBeNull();
    expect(submit).not.toHaveBeenCalled();
    expect(batch).not.toHaveBeenCalled();
    expect(production).not.toHaveBeenCalled();
  });

  it("builds that need no payment decision end the plan at the builder", async () => {
    const preview = engineLike("pok6warfare", {
      yes: previewReply({
        status: "question",
        choice: engineProduction([{ unit: "infantry", cost: 1, count: 2 }]),
        step: 1,
      }),
      [`yes>${build("infantry")}`]: previewReply({ status: "complete" }),
    });
    setup("pok6warfare", "Warfare", { preview });
    await open();
    await act(async () => {
      click("secondary-yes-btn");
    });
    await settle();
    fireEvent.click(screen.getByTestId(`produce-unit-btn-${build("infantry")}`));
    fireEvent.click(screen.getByRole("button", { name: "Save these builds" }));
    await settle();
    expect(plan()?.production).toEqual({ builds: ["infantry"] });
    expect(screen.queryByTestId("prepare-banner")).toBeNull();
  });

  it("without the engine's preview Warfare can only be followed or skipped (no estimate of a build list)", async () => {
    const preview = vi.fn().mockRejectedValue(new PreviewUnsupportedError());
    const { production } = setup("pok6warfare", "Warfare", { preview });
    await open();
    await act(async () => {
      click("secondary-yes-btn");
    });
    await settle();
    expect(plan()).toEqual({ card: "pok6warfare", follow: true });
    expect(screen.queryByTestId("production-builder-drawer")).toBeNull();
    expect(screen.queryByTestId("prepare-banner")).toBeNull();
    expect(production).not.toHaveBeenCalled();
  });

  describe("when the real question opens", () => {
    const realWindow = (): PendingChoiceDto =>
      secondaryChoice("pok6warfare", { prompt: "spend a strategy token to produce at home" });
    const realProduction = (over?: Parameters<typeof engineProduction>[1]): PendingChoiceDto =>
      pendingFromEngine(
        engineProduction(
          [
            { unit: "infantry", cost: 1, count: 2 },
            { unit: "carrier", cost: 3, count: 1 },
          ],
          over,
        ),
        "n-prod",
      );

    const planned = async (mode: "review" | "auto") => {
      localStorage.setItem(SECONDARY_PREP_MODE_KEY, mode);
      const ctx = await prepare();
      fireEvent.click(screen.getByTestId(`produce-unit-btn-${build("infantry")}`));
      fireEvent.click(screen.getByRole("button", { name: "Save these builds" }));
      await settle();
      fireEvent.click(screen.getByTestId("planet-card-exhaust|jord"));
      await act(async () => {
        click("confirm-payment-btn");
      });
      await settle();
      return ctx;
    };

    it("Review: the real builder opens with the builds staged and one Confirm sends them the builder's way", async () => {
      const { again, production, batch, submit } = await planned("review");
      again({ history: hist(6), choice: realWindow() });
      await settle();
      // The window question is answered by hand here (Review): its prepared answer is the follow.
      await act(async () => {
        click("secondary-prepared-confirm");
      });
      expect(submit).toHaveBeenCalledWith("yes");
      again({ history: hist(7), choice: realProduction() });
      await settle();
      expect(screen.getByTestId("production-builder-drawer")).toBeInTheDocument();
      // Prefill: the plan's infantry is already staged in the real builder.
      expect(screen.getByTestId(`produce-count-${build("infantry")}`).textContent).toBe("1");
      expect(screen.getByTestId("secondary-prepared-bar").textContent).toMatch(/Build infantry/);
      await act(async () => {
        click("secondary-prepared-confirm");
      });
      expect(production).toHaveBeenCalledTimes(1);
      expect(production.mock.calls[0][0]).toEqual(["infantry"]);
      expect(production.mock.calls[0][1]).toBe("18");
      expect(batch).not.toHaveBeenCalled();
      // Then the payment the engine asks for the first build is the prepared one.
      again({ history: hist(8), choice: pendingFromEngine(enginePayment(1), "n-pay") });
      await settle();
      expect(screen.getByTestId("secondary-prepared-bar").textContent).toMatch(/Pay with Jord/);
      await act(async () => {
        click("secondary-prepared-confirm");
      });
      expect(batch).toHaveBeenCalledWith({
        kind: "payment",
        steps: [{ kind: "exhaust", planet: "jord" }],
      });
    });

    it("Needs review when the builds no longer fit, and nothing is sent", async () => {
      const { again, production } = await planned("review");
      again({ history: hist(6), choice: realWindow() });
      await settle();
      await act(async () => {
        click("secondary-prepared-confirm");
      });
      again({ history: hist(7), choice: realProduction({ capacity: 1 }) });
      await settle();
      expect(screen.getByTestId("secondary-needs-review").textContent).toMatch(/no longer fit the production capacity/);
      expect(screen.queryByTestId("secondary-prepared-confirm")).toBeNull();
      expect(production).not.toHaveBeenCalled();
    });

    it("Auto: the builds are sent without opening the builder, then the payment, each after the visible delay", async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const { again, production, batch } = await planned("auto");
      again({ history: hist(6), choice: realWindow() });
      await settle();
      await act(async () => {
        await vi.advanceTimersByTimeAsync(AUTO_PLAY_DELAY_MS + 50);
      });
      again({ history: hist(7), choice: realProduction() });
      await settle();
      expect(screen.getByTestId("secondary-autoplay-toast").textContent).toMatch(/Build infantry/);
      expect(screen.queryByTestId("production-builder-drawer")).toBeNull();
      await act(async () => {
        await vi.advanceTimersByTimeAsync(AUTO_PLAY_DELAY_MS + 50);
      });
      expect(production).toHaveBeenCalledTimes(1);
      again({ history: hist(8), choice: pendingFromEngine(enginePayment(1), "n-pay") });
      await settle();
      await act(async () => {
        await vi.advanceTimersByTimeAsync(AUTO_PLAY_DELAY_MS + 50);
      });
      expect(batch).toHaveBeenCalledWith({ kind: "payment", steps: [{ kind: "exhaust", planet: "jord" }] });
    });

    it("a failed send is not marked done: the plan can be confirmed again", async () => {
      const { again, production } = await planned("review");
      again({ history: hist(6), choice: realWindow() });
      await settle();
      await act(async () => {
        click("secondary-prepared-confirm");
      });
      again({ history: hist(7), choice: realProduction() });
      await settle();
      production.mockRejectedValueOnce(new Error("The production decision is no longer open"));
      await act(async () => {
        click("secondary-prepared-confirm");
      });
      await settle();
      expect(production).toHaveBeenCalledTimes(1);
      await act(async () => {
        click("secondary-prepared-confirm");
      });
      expect(production).toHaveBeenCalledTimes(2);
    });

    it("another decision of the same action is not answered by the plan", async () => {
      const { again, production } = await planned("review");
      again({ history: hist(6), choice: realWindow() });
      await settle();
      await act(async () => {
        click("secondary-prepared-confirm");
      });
      again({ history: hist(7), choice: stepChoice("place_unit", [{ id: "place|x", kind: "place", label: "x" }]) });
      await settle();
      expect(screen.queryByTestId("secondary-prepared-bar")).toBeNull();
      expect(screen.queryByTestId("secondary-needs-review")).toBeNull();
      expect(production).not.toHaveBeenCalled();
    });
  });
});
