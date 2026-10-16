import React, { useState } from "react";
import { afterEach, beforeEach, describe, expect, it, vi } from "vitest";
import { act, fireEvent, render, screen } from "@testing-library/react";
import type { GameEvent, HistoryStatus, PendingChoiceDto, PlayerView } from "../protocol/types.ts";
import type { BasketPlan, SecondaryPreviewReply } from "../protocol/client.ts";
import { ChoiceRendererDispatcher } from "./GameShell.tsx";
import { SecondaryPrepHost } from "./SecondaryPrepHost.tsx";
import { useSecondaryPrepare, useTokenPrefill } from "../hooks/useSecondaryPrepare.ts";
import { PreparedHintProvider } from "../presentation/PreparedHint.tsx";
import {
  CommandTokenDraftProvider,
  useCommandTokenDraft,
} from "../presentation/CommandTokenDraftContext.tsx";
import { SECONDARY_PREP_MODE_KEY } from "../hooks/useSecondaryPrepMode.ts";
import { AUTO_PLAY_DELAY_MS } from "../hooks/useSecondaryAutoPlay.ts";
import { loadPlan } from "../hooks/useSecondaryPlan.ts";
import { leadershipScript } from "../presentation/secondaryPreview.ts";
import {
  boardWith,
  engineWindow,
  playedLog,
  player,
  previewReply,
  secondaryChoice,
} from "../test/secondaryPrepFixtures.ts";

const hist = (cursor: number, generation = 0): HistoryStatus => ({ cursor, redo_count: 0, generation });

type Preview = (card: string, primary: string, answers: readonly string[]) => Promise<SecondaryPreviewReply>;

const CARD = "pok1leadership";

/** The Leadership window as the engine sends it: the purchase facts ride on it. */
const purchaseDetails = (planets: { id: string; worth: number }[], tradeGoods: number) => ({
  mode: "buy",
  costs_token: false,
  pools: { tactic: 3, fleet: 3, strategic: 2 },
  tokens_to_place: 0,
  reinforcements: 10,
  purchase: {
    cost: 3,
    influence_available: planets.reduce((sum, planet) => sum + planet.worth, 0) + tradeGoods,
    max: Math.floor((planets.reduce((sum, planet) => sum + planet.worth, 0) + tradeGoods) / 3),
    trade_goods: tradeGoods,
    trade_good_worth: 1,
    planets,
  },
});

const PLANETS = [
  { id: "jord", worth: 2 },
  { id: "lodor", worth: 1 },
];

const exactWindow = (planets = PLANETS, tradeGoods = 2) => ({
  ...engineWindow(CARD, "spend 3 influence for a command token", purchaseDetails(planets, tradeGoods)),
});

/** The real window question once it opens for the follower. */
const realWindow = (planets = PLANETS, tradeGoods = 2): PendingChoiceDto => ({
  ...secondaryChoice(CARD, { prompt: "spend 3 influence for a command token", nonce: "n-lead" }),
  details: { kind: "strategy_secondary", card: CARD, played_by: "a", tokens_left: 0, ...purchaseDetails(planets, tradeGoods) },
  context: { subtype: "buy_token_with_influence" } as PendingChoiceDto["context"],
});

interface HarnessProps {
  events: GameEvent[];
  choice?: PendingChoiceDto | null;
  history?: HistoryStatus;
  preview?: Preview;
  submit: (optionId: string) => Promise<void>;
  batch: (plan: BasketPlan) => Promise<void>;
}

/** The hook, the real decision renderer, the shared token draft and the prepare chrome, as App.tsx wires them. */
const Harness: React.FC<HarnessProps> = (p) => {
  const list: PlayerView[] = [
    player("a", { strategy_cards: [CARD] }),
    player("b", { trade_goods: 2 }),
    player("c"),
  ];
  const board = boardWith([
    { system: "18", planet: "jord", owner: "b" },
    { system: "26", planet: "lodor", owner: "b" },
  ]);
  const prep = useSecondaryPrepare({
    gameId: "g1",
    viewerSeat: "b",
    players: list,
    events: p.events,
    board,
    phase: "action",
    history: p.history ?? hist(5),
    realChoice: p.choice ?? null,
    submitChoice: p.submit,
    submitBatch: p.batch,
    previewSecondary: p.preview,
  });
  const tokenDraft = useCommandTokenDraft(prep.shownChoice);
  useTokenPrefill(prep, prep.shownChoice?.nonce, tokenDraft);
  const [minimized, setMinimized] = useState(false);
  const [selected, setSelected] = useState<string | undefined>();
  const record = Object.fromEntries(list.map((entry) => [entry.id, entry]));
  const marks = tokenDraft.mapPayment?.draft;
  return (
    <PreparedHintProvider value={null}>
      <CommandTokenDraftProvider value={tokenDraft}>
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
        {/* Stands in for the map: App.tsx's handleSelectTarget runs the same togglePlanet / stepTradeGoods. */}
        <button type="button" data-testid="map-toggle-lodor" onClick={() => tokenDraft.togglePlanet("lodor")} />
        <button type="button" data-testid="map-goods-plus" onClick={() => tokenDraft.stepTradeGoods(1)} />
        <output data-testid="map-marks">
          {marks ? `${marks.planetIds.join(",")}|${marks.tradeGoods}` : "none"}
        </output>
        <SecondaryPrepHost prep={prep} />
      </CommandTokenDraftProvider>
    </PreparedHintProvider>
  );
};

function setup(over: Partial<HarnessProps> = {}) {
  const submit = vi.fn().mockResolvedValue(undefined);
  const batch = vi.fn().mockResolvedValue(undefined);
  const props: HarnessProps = { events: playedLog("Leadership"), submit, batch, ...over };
  const view = render(<Harness {...props} />);
  const again = (next: Partial<HarnessProps>) => {
    Object.assign(props, next);
    view.rerender(<Harness {...props} />);
  };
  return { submit, batch, again };
}

const click = (id: string) => fireEvent.click(screen.getByTestId(id));
const settle = () => act(async () => {});
const plan = () => loadPlan("g1", "b")?.plan;
const open = async () => {
  await act(async () => {
    click("secondary-prep-chip");
  });
  await settle();
};

/** An engine preview: the exact window; the scripted purchase is accepted, or rejected at `at`. */
function enginePreview(verdict: "accept" | "reject" | "unavailable" = "accept") {
  return vi.fn(async (_card: string, _primary: string, answers: readonly string[]) => {
    if (answers.length === 0) {
      return previewReply({ status: "question", choice: exactWindow(), step: 0 });
    }
    if (verdict === "unavailable") return previewReply({ status: "unavailable", detail: "nope" });
    if (verdict === "reject") {
      return previewReply({
        status: "rejected",
        at: 1,
        answer: answers[1],
        choice: {
          player: "b",
          prompt: "pay 3 more influence",
          options: [{ id: "exhaust|jord", kind: "pay", label: "jord" }],
          context: { subtype: "pay_influence" } as never,
        },
      });
    }
    return previewReply({ status: "complete" });
  }) as Preview & ReturnType<typeof vi.fn>;
}

beforeEach(() => {
  localStorage.clear();
  localStorage.removeItem(SECONDARY_PREP_MODE_KEY);
});
afterEach(() => vi.useRealTimers());

describe("Leadership: preparing the exact payment", () => {
  /** Planet first: picks jord, tops it up with a trade good (so one token is bought), assigns it to the fleet pool. */
  const chooseAndSave = async () => {
    // Nothing is staged when the panel opens; the player starts from the planets.
    expect(screen.getByTestId("token-buy-count").textContent).toBe("0");
    expect(screen.getByTestId("map-marks").textContent).toBe("|0");
    click("token-payment-planet-jord");
    expect(screen.getByTestId("token-buy-count").textContent).toBe("0");
    click("map-goods-plus");
    expect(screen.getByTestId("token-buy-count").textContent).toBe("1");
    expect(screen.getByTestId("map-marks").textContent).toBe("exhaust|jord|1");
    click("token-plus-fleet");
    await act(async () => {
      click("token-confirm");
    });
    await settle();
  };

  it("pays on the map in preparation, has the engine check the whole purchase, and saves planets and goods", async () => {
    const preview = enginePreview();
    const { submit, batch } = setup({ preview });
    await open();
    expect(screen.getByTestId("prepare-leadership-payment")).toBeInTheDocument();
    await chooseAndSave();
    expect(plan()).toEqual({
      card: CARD,
      follow: true,
      leadership: {
        pools: { tactic: 0, fleet: 1, strategic: 0 },
        payment: { planets: ["jord"], tradeGoods: 1 },
      },
    });
    // The engine was asked with the exact answers the real flow takes: window yes, the payments, the pool.
    expect(preview).toHaveBeenCalledWith(CARD, "a", ["yes", "exhaust|jord", "trade_good", "fleet_tokens"]);
    expect(submit).not.toHaveBeenCalled();
    expect(batch).not.toHaveBeenCalled();
  });

  it("a payment the engine does not offer is refused with its reason and nothing is saved", async () => {
    const { batch } = setup({ preview: enginePreview("reject") });
    await open();
    await chooseAndSave();
    expect(plan()).toBeUndefined();
    expect(screen.getByTestId("token-error").textContent).toMatch(/would not accept this purchase/i);
    expect(screen.getByTestId("token-error").textContent).toMatch(/Jord cannot be exhausted/);
    // What was staged stays, so the player can change the payment.
    expect(screen.getByTestId("token-buy-count").textContent).toBe("1");
    expect(batch).not.toHaveBeenCalled();
  });

  it("without an answer from the engine the payment is saved anyway and the note says it was not checked", async () => {
    setup({ preview: enginePreview("unavailable") });
    await open();
    await chooseAndSave();
    expect(plan()?.leadership?.payment).toEqual({ planets: ["jord"], tradeGoods: 1 });
  });

  it("scripts the engine's answers from a staged purchase: first yes is the window's, later ones are 'yes'", () => {
    expect(
      leadershipScript("yes", [
        { kind: "purchase", buy: true },
        { kind: "exhaust", planet: "jord" },
        { kind: "pool", pool: "tactic_tokens" },
        { kind: "purchase", buy: true },
        { kind: "trade_good" },
        { kind: "trade_good" },
        { kind: "trade_good" },
        { kind: "pool", pool: "fleet_tokens" },
        { kind: "purchase", buy: false },
      ]),
    ).toEqual(["yes", "exhaust|jord", "tactic_tokens", "yes", "trade_good", "trade_good", "trade_good", "fleet_tokens"]);
  });

  describe("when the real question opens", () => {
    const planned = async (mode: "review" | "auto") => {
      localStorage.setItem(SECONDARY_PREP_MODE_KEY, mode);
      const ctx = setup({ preview: enginePreview() });
      await open();
      await chooseAndSave();
      return ctx;
    };

    it("Review: the usual panel opens with the tokens, pool and payment already chosen; Confirm sends that payment", async () => {
      const { again, batch } = await planned("review");
      again({ history: hist(6), choice: realWindow() });
      await settle();
      expect(screen.getByTestId("secondary-prepared-bar").textContent).toMatch(/Buy 1 command token \(1 fleet\).*paying Jord, 1 trade good/);
      // Prefilled: one token bought, in the fleet pool, and the map shows jord + a trade good, not Auto-pay's.
      expect(screen.getByTestId("token-buy-count").textContent).toBe("1");
      expect(screen.getByTestId("token-count-fleet").textContent).toBe("4"); // 3 held + the bought one
      expect(screen.getByTestId("map-marks").textContent).toBe("exhaust|jord|1");
      await act(async () => {
        click("secondary-prepared-confirm");
      });
      expect(batch).toHaveBeenCalledTimes(1);
      const sent = batch.mock.calls[0][0] as { kind: string; steps: { kind: string; planet?: string }[] };
      expect(sent.kind).toBe("tokens");
      expect(sent.steps).toEqual([
        { kind: "purchase", buy: true },
        { kind: "exhaust", planet: "jord" },
        { kind: "trade_good" },
        { kind: "pool", pool: "fleet_tokens" },
      ]);
    });

    it("Auto: the prepared payment is sent after the visible delay", async () => {
      vi.useFakeTimers({ shouldAdvanceTime: true });
      const { again, batch } = await planned("auto");
      again({ history: hist(6), choice: realWindow() });
      await settle();
      expect(screen.getByTestId("secondary-autoplay-toast").textContent).toMatch(/paying Jord, 1 trade good/);
      await act(async () => {
        await vi.advanceTimersByTimeAsync(AUTO_PLAY_DELAY_MS + 50);
      });
      expect(batch).toHaveBeenCalledTimes(1);
      expect((batch.mock.calls[0][0] as { steps: unknown[] }).steps).toContainEqual({ kind: "exhaust", planet: "jord" });
    });

    it("Needs review when a prepared planet was exhausted meanwhile: the reason, Auto-pay's replacement selected, nothing sent", async () => {
      const { again, batch } = await planned("review");
      // jord is no longer offered (an earlier follower or the primary used it); lodor and tarra remain.
      again({
        history: hist(6),
        choice: realWindow([{ id: "lodor", worth: 2 }, { id: "tarra", worth: 1 }], 2),
      });
      await settle();
      const review = screen.getByTestId("secondary-needs-review").textContent ?? "";
      expect(review).toMatch(/Jord can no longer pay/);
      expect(review).toMatch(/Auto-pay would use/);
      expect(screen.queryByTestId("secondary-prepared-confirm")).toBeNull();
      // The replacement is selected in the usual panel for the player to confirm (one token, Auto-pay's payment).
      expect(screen.getByTestId("token-buy-count").textContent).toBe("1");
      expect(batch).not.toHaveBeenCalled();
    });

    it("Needs review when the trade goods were spent meanwhile", async () => {
      const { again, batch } = await planned("review");
      again({ history: hist(6), choice: realWindow(PLANETS, 0) });
      await settle();
      expect(screen.getByTestId("secondary-needs-review").textContent).toMatch(/trade good/);
      expect(batch).not.toHaveBeenCalled();
    });
  });
});
