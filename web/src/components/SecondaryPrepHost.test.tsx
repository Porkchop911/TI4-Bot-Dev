import React, { useState } from "react";
import { describe, it, expect, beforeEach, afterEach, vi } from "vitest";
import { act, fireEvent, render, screen, within } from "@testing-library/react";
import type {
  BoardView,
  GameEvent,
  HistoryStatus,
  PendingChoiceDto,
  PlayerView,
} from "../protocol/types.ts";
import type { BasketPlan } from "../protocol/client.ts";
import { ChoiceRendererDispatcher } from "./GameShell.tsx";
import { SecondaryPrepHost } from "./SecondaryPrepHost.tsx";
import { PlayerSheet } from "./PlayerSheet.tsx";
import { useReadyPrefill, useSecondaryPrepare } from "../hooks/useSecondaryPrepare.ts";
import {
  ReadyPlanetsDraftProvider,
  useReadyPlanetsDraft,
} from "../presentation/ReadyPlanetsDraftContext.tsx";
import { resolveMapTargetSelection } from "../presentation/planetSelection.ts";
import { PreparedHintProvider } from "../presentation/PreparedHint.tsx";
import { SECONDARY_PREP_MODE_KEY } from "../hooks/useSecondaryPrepMode.ts";
import { AUTO_PLAY_DELAY_MS, AUTO_PLAY_HOLD_LIMIT_MS } from "../hooks/useSecondaryAutoPlay.ts";
import { loadPlan } from "../hooks/useSecondaryPlan.ts";
import {
  actionEvent,
  boardWith,
  option,
  playedLog,
  player,
  secondaryChoice,
  stepChoice,
} from "../test/secondaryPrepFixtures.ts";

const hist = (cursor: number, generation = 0, redo_count = 0): HistoryStatus => ({ cursor, redo_count, generation });
const players = (card = "pok7technology", extra: Partial<PlayerView> = {}): PlayerView[] => [
  player("a", { strategy_cards: [card, "pok1leadership"] }),
  player("b", { trade_goods: 4, ...extra }),
  player("c"),
];

interface HarnessProps {
  viewerSeat?: string | null;
  players?: PlayerView[];
  events?: GameEvent[];
  board?: BoardView;
  phase?: string;
  history?: HistoryStatus;
  choice?: PendingChoiceDto | null;
  submit: (optionId: string) => Promise<void>;
  batch: (plan: BasketPlan) => Promise<void>;
}

/** Wires the hook, the real decision renderer and the prepare chrome the way App.tsx does. */
const Harness: React.FC<HarnessProps> = (p) => {
  const viewerSeat = p.viewerSeat === undefined ? "b" : p.viewerSeat;
  const list = p.players ?? players();
  const board = p.board ?? boardWith([{ system: "18", planet: "jord", owner: "b", exhausted: true }]);
  const prep = useSecondaryPrepare({
    gameId: "g1",
    viewerSeat,
    players: list,
    events: p.events ?? playedLog("Technology"),
    board,
    phase: p.phase ?? "action",
    history: p.history ?? hist(5),
    realChoice: p.choice ?? null,
    submitChoice: p.submit,
    submitBatch: p.batch,
  });
  // Diplomacy's picks are shared by the bar and the map, as in App.tsx.
  const ready = useReadyPlanetsDraft(prep.shownChoice, board);
  useReadyPrefill(prep, prep.shownChoice?.nonce, ready);
  const [minimized, setMinimized] = useState(false);
  const [selected, setSelected] = useState<string | undefined>();
  const [planet, setPlanet] = useState<string | null>(null);
  const record = Object.fromEntries(list.map((entry) => [entry.id, entry]));
  return (
    <PreparedHintProvider
      value={
        prep.resolution.kind === "option" && !prep.preparing && prep.realChoice
          ? { optionId: prep.resolution.optionId, text: prep.resolution.text }
          : null
      }
    >
      <ReadyPlanetsDraftProvider value={ready}>
      <ChoiceRendererDispatcher
        choice={prep.shownChoice}
        viewerSeat={viewerSeat}
        players={record}
        boardView={board}
        onSubmit={prep.preparing ? prep.prepareSubmit : p.submit}
        onSubmitBasketBatch={prep.preparing ? prep.prepareBatch : p.batch}
        isMinimized={minimized}
        onMinimizedChange={setMinimized}
        selectedOptionId={selected}
        onSelectOption={setSelected}
        selectedPlanetId={planet}
        onSelectPlanet={setPlanet}
      />
      {/* Stands in for a click on the map: App.tsx runs the same resolveMapTargetSelection. */}
      <button
        type="button"
        data-testid="map-click-jord"
        onClick={() => {
          if (ready.view) return ready.toggle("jord");
          const picked = resolveMapTargetSelection(prep.shownChoice, "18", "jord", board);
          if (picked.kind === "select") {
            setSelected(picked.optionId);
            setPlanet(picked.planetId ?? null);
          }
        }}
      />
      <button type="button" data-testid="map-click-lodor" onClick={() => ready.toggle("lodor")} />
      <SecondaryPrepHost
        prep={prep}
        onConfirm={async (resolution) => {
          if (resolution.kind === "option") await p.submit(resolution.optionId);
          else if (resolution.kind === "tokens") await p.batch({ kind: "tokens", steps: resolution.steps });
          else if (resolution.kind === "ready") await prep.play(resolution);
        }}
      />
      </ReadyPlanetsDraftProvider>
    </PreparedHintProvider>
  );
};

function setup(over: Partial<HarnessProps> = {}) {
  const submit = vi.fn().mockResolvedValue(undefined);
  const batch = vi.fn().mockResolvedValue(undefined);
  const props: HarnessProps = { submit, batch, ...over };
  const view = render(<Harness {...props} />);
  const again = (next: Partial<HarnessProps>) => {
    Object.assign(props, next);
    view.rerender(<Harness {...props} />);
  };
  return { submit, batch, props, view, again };
}

const click = (id: string) => fireEvent.click(screen.getByTestId(id));
const open = () => click("secondary-prep-chip");
const plan = () => loadPlan("g1", "b")?.plan;
/** The first technology the real list offers, picked and confirmed through the real controls. */
const pickFirstTech = async () => {
  const card = document.querySelector('[data-testid^="tech-card-"][data-selectable="true"]') as HTMLElement;
  const id = card.getAttribute("data-testid")!.replace("tech-card-", "");
  fireEvent.click(card);
  await act(async () => {
    click("confirm-research-btn");
  });
  await act(async () => {});
  return id;
};
const dialog = () => screen.queryByTestId("pending-choice-dialog");

describe("waiting: the chip", () => {
  beforeEach(() => localStorage.clear());

  it("offers 'Prepare your secondary' to an eligible follower who has not been asked", () => {
    setup();
    expect(screen.getByTestId("secondary-prep-chip").textContent).toMatch(/Prepare your secondary/);
    expect(dialog()).toBeNull();
  });

  it("shows nothing to spectators, the primary player, tokenless followers, or once asked", () => {
    const hidden: Partial<HarnessProps>[] = [
      { viewerSeat: null },
      { viewerSeat: "a" },
      { players: players("pok7technology", { strategic_tokens: 0 }) },
      { events: [...playedLog("Technology"), actionEvent("action_5", "b", { detail: "b followed" })] },
      { events: [] },
      { phase: "status" },
    ];
    for (const over of hidden) {
      const { view } = setup(over);
      expect(screen.queryByTestId("secondary-prep")).toBeNull();
      view.unmount();
    }
  });

  it("hides the chip while the viewer has a real decision pending", () => {
    setup({ choice: stepChoice("reaction_when_x", [option("a")]) });
    expect(screen.queryByTestId("secondary-prep-chip")).toBeNull();
  });
});

describe("preparation mode renders the REAL components and sends nothing", () => {
  beforeEach(() => localStorage.clear());

  it("shows the banner and the real strategy secondary panel for a dry choice", () => {
    const { submit, batch } = setup();
    open();
    expect(screen.getByTestId("prepare-banner").textContent).toMatch(/Preparing\s*—\s*nothing is sent or spent/);
    expect(screen.getByTestId("strategy-secondary-panel")).toBeInTheDocument();
    expect(screen.getByTestId("secondary-yes-btn").textContent).toMatch(/Spend 1 strategy token \+ 4 to research/);
    expect(within(screen.getByTestId("secondary-yes-btn")).getByRole("img", { name: "4 resources" })).toBeInTheDocument();
    expect(submit).not.toHaveBeenCalled();
    expect(batch).not.toHaveBeenCalled();
  });

  it("folds the banner on a phone (state only: the styles do the folding) and keeps Save plan", () => {
    setup();
    open();
    const banner = screen.getByTestId("prepare-banner");
    const fold = screen.getByTestId("prep-fold");
    expect(banner.dataset.folded).toBe("false");
    expect(fold.getAttribute("aria-expanded")).toBe("true");
    fireEvent.click(fold);
    expect(banner.dataset.folded).toBe("true");
    expect(fold.getAttribute("aria-expanded")).toBe("false");
    expect(fold.getAttribute("aria-label")).toBe("Show details");
    expect(screen.getByTestId("prep-save")).toBeInTheDocument();
    // The strategy question itself stays mounted: folding only changes the banner.
    expect(screen.getByTestId("strategy-secondary-panel")).toBeInTheDocument();
    fireEvent.click(fold);
    expect(banner.dataset.folded).toBe("false");
    // Closing resets the fold for the next time.
    fireEvent.click(fold);
    click("prep-save");
    open();
    expect(screen.getByTestId("prepare-banner").dataset.folded).toBe("false");
  });

  it("Technology: follow, then the real technology list; picking records the plan (no submit_choice)", async () => {
    const { submit, batch } = setup();
    open();
    click("secondary-yes-btn");
    expect(plan()).toEqual({ card: "pok7technology", follow: true });
    expect(screen.getByTestId("prepare-approximate").textContent).toMatch(/Approximate until the real question opens/);
    const tech = await pickFirstTech();
    expect(plan()).toEqual({ card: "pok7technology", follow: true, tech });
    expect(screen.queryByTestId("prepare-banner")).toBeNull();
    expect(screen.getByTestId("secondary-prep-chip").textContent).toMatch(/Prepared/);
    expect(submit).not.toHaveBeenCalled();
    expect(batch).not.toHaveBeenCalled();
  });

  it("skip records a skip and ends", () => {
    const { submit } = setup();
    open();
    click("secondary-skip-btn");
    expect(plan()).toEqual({ card: "pok7technology", follow: false });
    expect(screen.queryByTestId("prepare-banner")).toBeNull();
    expect(submit).not.toHaveBeenCalled();
  });

  it("Save plan keeps the plan, Clear plan revokes it", () => {
    setup();
    open();
    click("secondary-yes-btn");
    click("prep-save");
    expect(plan()).toBeTruthy();
    open();
    click("prep-clear");
    expect(plan()).toBeUndefined();
    expect(screen.queryByTestId("prepare-banner")).toBeNull();
  });

  it("Diplomacy: both planets are picked on the real ready bar (list and map) and recorded in one confirm", async () => {
    const board = boardWith([
      { system: "18", planet: "jord", owner: "b", exhausted: true },
      { system: "26", planet: "lodor", owner: "b", exhausted: true },
      { system: "27", planet: "quann", owner: "b", exhausted: true },
    ]);
    const { submit, batch } = setup({ players: players("pok2diplomacy"), events: playedLog("Diplomacy"), board });
    open();
    click("secondary-yes-btn");
    expect(screen.getByTestId("ready-planets-bar")).toBeInTheDocument();
    expect(screen.queryByTestId("planet-selection-bar")).toBeNull();
    expect((screen.getByTestId("ready-bar-confirm") as HTMLButtonElement).disabled).toBe(true);
    // A map click and a list tap pick the same way.
    click("map-click-jord");
    expect(screen.getByTestId("ready-planet-chip-jord").getAttribute("aria-pressed")).toBe("true");
    expect((screen.getByTestId("ready-bar-confirm") as HTMLButtonElement).disabled).toBe(true);
    click("ready-planet-chip-lodor");
    expect(screen.getByTestId("ready-bar-count").textContent).toMatch(/2 of 2/);
    await act(async () => {
      click("ready-bar-confirm");
    });
    expect(plan()?.planets).toEqual(["jord", "lodor"]);
    expect(screen.queryByTestId("prepare-banner")).toBeNull();
    // Nothing is sent while preparing.
    expect(submit).not.toHaveBeenCalled();
    expect(batch).not.toHaveBeenCalled();
  });

  it("Diplomacy: arriving with a prepared pair opens the bar with both picked; Confirm sends one batch", async () => {
    const board = boardWith([
      { system: "18", planet: "jord", owner: "b", exhausted: true },
      { system: "26", planet: "lodor", owner: "b", exhausted: true },
      { system: "27", planet: "quann", owner: "b", exhausted: true },
    ]);
    const { batch, submit, again } = setup({ players: players("pok2diplomacy"), events: playedLog("Diplomacy"), board });
    open();
    click("secondary-yes-btn");
    click("map-click-jord");
    click("ready-planet-chip-quann");
    await act(async () => {
      click("ready-bar-confirm");
    });
    expect(plan()?.planets).toEqual(["jord", "quann"]);
    // The secondary window opens first (the plan applies from there), then the follow-up question.
    again({ history: hist(6), choice: secondaryChoice("pok2diplomacy") });
    again({
      history: hist(7),
      choice: stepChoice(
        "ready_planet",
        [option("jord", "ready", "ready jord"), option("lodor", "ready", "ready lodor"), option("quann", "ready", "ready quann")],
        "ready-1",
      ),
    });
    expect(screen.getByTestId("ready-planet-chip-jord").getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByTestId("ready-planet-chip-quann").getAttribute("aria-pressed")).toBe("true");
    expect(screen.getByTestId("ready-planet-chip-lodor").getAttribute("aria-pressed")).toBe("false");
    await act(async () => {
      click("ready-bar-confirm");
    });
    expect(batch).toHaveBeenCalledTimes(1);
    expect(batch).toHaveBeenCalledWith({
      kind: "ready_planets",
      steps: [
        { kind: "ready_planet", planet: "jord" },
        { kind: "ready_planet", planet: "quann" },
      ],
    });
    expect(submit).not.toHaveBeenCalled();
  });

  it("Leadership: the real command token panel, pool arrangement recorded in the plan", async () => {
    const lead = players("pok1leadership", { trade_goods: 3 });
    const { submit, batch } = setup({
      players: lead,
      events: playedLog("Leadership"),
      board: boardWith([{ system: "18", planet: "jord", owner: "b" }]),
    });
    open();
    expect(screen.getByTestId("command-token-panel")).toBeInTheDocument();
    click("token-buy-plus");
    click("token-plus-fleet");
    await act(async () => {
      click("token-confirm");
    });
    expect(plan()).toEqual({
      card: "pok1leadership",
      follow: true,
      // The payment on the map (here Auto-pay's) is part of the plan now.
      leadership: { pools: { tactic: 0, fleet: 1, strategic: 0 }, payment: { planets: ["jord"], tradeGoods: 1 } },
    });
    expect(submit).not.toHaveBeenCalled();
    expect(batch).not.toHaveBeenCalled();
  });

  it("Trade: only follow or skip, with the replenish warning", () => {
    setup({ players: players("pok5trade"), events: playedLog("Trade") });
    open();
    expect(screen.getByTestId("prepare-approximate").textContent).toMatch(/replenishes you/);
    click("secondary-yes-btn");
    expect(plan()).toEqual({ card: "pok5trade", follow: true });
    expect(screen.queryByTestId("prepare-banner")).toBeNull();
  });

  it("the mode ends by itself when the viewer's real decision arrives", () => {
    const { again } = setup();
    open();
    expect(screen.getByTestId("prepare-banner")).toBeInTheDocument();
    again({ history: hist(6), choice: secondaryChoice("pok7technology") });
    expect(screen.queryByTestId("prepare-banner")).toBeNull();
    expect(dialog()).toBeInTheDocument();
  });
});

describe("invalidation", () => {
  beforeEach(() => localStorage.clear());

  const prepared = () => {
    const ctx = setup();
    open();
    click("secondary-yes-btn");
    click("prep-save");
    return ctx;
  };

  it("drops the plan on a new strategic action", () => {
    const { again } = prepared();
    again({
      events: [...playedLog("Technology"), actionEvent("action_9", "c", { action_actor: "c", detail: "c played Politics" })],
      players: [player("a"), player("b"), player("c", { strategy_cards: ["pok3politics"] })],
    });
    expect(plan()).toBeUndefined();
  });

  it("drops the plan when the action is cancelled (Coup d'Etat)", () => {
    const { again } = prepared();
    again({ events: [...playedLog("Technology"), actionEvent("action_5", "c", { detail: "c played Coup d'Etat" })] });
    expect(plan()).toBeUndefined();
  });

  it("keeps the plan through the same action's later decisions of other seats", () => {
    const { again } = prepared();
    expect(plan()).toBeDefined();
    again({
      events: [
        ...playedLog("Technology"),
        actionEvent("action_5", "a", { stage: "strategy", detail: "a researched" }),
        actionEvent("action_5", "c", { stage: "strategy", detail: "c followed" }),
      ],
    });
    expect(plan()).toBeDefined();
  });

  it("keeps the plan when a batch commit bumps the history generation mid-action", async () => {
    const { again } = prepared();
    again({ history: hist(3, 1) });
    expect(plan()).toBeDefined();
  });

  it("drops the plan when an undo took the action out of the log", async () => {
    const { again } = prepared();
    again({ history: hist(3, 1), events: [actionEvent("action_3", "a", { action_type: "tactical" })] });
    expect(plan()).toBeUndefined();
  });
});

describe("review mode (the default): the prepared answer in the real decision", () => {
  beforeEach(() => localStorage.clear());

  async function arrive(choice: PendingChoiceDto) {
    const ctx = setup();
    open();
    click("secondary-yes-btn");
    const tech = await pickFirstTech();
    ctx.again({ history: hist(6), choice });
    return { ...ctx, tech };
  }

  it("opens the REAL secondary panel with the prepared answer marked, and confirms with one click", async () => {
    const { submit } = await arrive(secondaryChoice("pok7technology"));
    expect(dialog()).toBeInTheDocument();
    expect(screen.getByTestId("strategy-secondary-panel")).toBeInTheDocument();
    expect(screen.getByTestId("secondary-yes-btn").getAttribute("data-prepared")).toBe("true");
    expect(screen.getByTestId("secondary-prepared-bar").textContent).toMatch(/Follow Technology/);
    expect(submit).not.toHaveBeenCalled();
    await act(async () => {
      click("secondary-prepared-confirm");
    });
    expect(submit).toHaveBeenCalledWith("yes");
  });

  it("the real yes button still answers directly", async () => {
    const { submit } = await arrive(secondaryChoice("pok7technology"));
    await act(async () => {
      click("secondary-yes-btn");
    });
    expect(submit).toHaveBeenCalledWith("yes");
  });

  it("lets the player choose differently without sending anything", async () => {
    const { submit } = await arrive(secondaryChoice("pok7technology"));
    click("secondary-prepared-dismiss");
    expect(screen.queryByTestId("secondary-prepared-bar")).toBeNull();
    expect(submit).not.toHaveBeenCalled();
  });

  it("uses the faction waiver option when that is what the engine offers", async () => {
    const { submit } = await arrive(
      secondaryChoice("pok7technology", {}, [option("no"), option("follow|waived|0|w", "strategy", "free")]),
    );
    await act(async () => {
      click("secondary-prepared-confirm");
    });
    expect(submit).toHaveBeenCalledWith("follow|waived|0|w");
  });

  it("shows 'Needs review' instead of pre-filling when the prepared intent no longer validates", async () => {
    await arrive(secondaryChoice("pok7technology", {}, [option("no")]));
    expect(screen.getByTestId("secondary-needs-review").textContent).toMatch(/Needs review/);
    expect(screen.queryByTestId("secondary-prepared-confirm")).toBeNull();
    expect(dialog()).toBeInTheDocument();
  });

  it("pre-fills the technology pick after the window, and flags an unavailable one", async () => {
    const { submit, again, props, tech } = await arrive(secondaryChoice("pok7technology"));
    const asked = [...(props.events ?? playedLog("Technology")), actionEvent("action_5", "b", { detail: "b followed" })];
    again({
      events: asked,
      history: hist(7),
      choice: stepChoice("research_technology", [option(tech, "research"), option("decline", "decline")]),
    });
    expect(screen.getByTestId("secondary-prepared-bar").textContent).toMatch(/Research/);
    await act(async () => {
      click("secondary-prepared-confirm");
    });
    expect(submit).toHaveBeenCalledWith(tech);
    again({ history: hist(8), choice: stepChoice("research_technology", [option("nm", "research")], "other") });
    expect(screen.getByTestId("secondary-needs-review")).toBeInTheDocument();
  });

  it("does not apply the plan to an unrelated research prompt after a reload", () => {
    const first = setup();
    open();
    click("secondary-yes-btn");
    click("prep-save");
    first.view.unmount();
    setup({ choice: stepChoice("research_technology", [option("amd", "research")]) });
    expect(screen.queryByTestId("secondary-prepared-bar")).toBeNull();
    expect(screen.queryByTestId("secondary-needs-review")).toBeNull();
  });

  it("never auto-plays in Review mode, even when the game moved forward", async () => {
    vi.useFakeTimers();
    try {
      const { submit } = await arrive(secondaryChoice("pok7technology"));
      act(() => {
        vi.advanceTimersByTime(AUTO_PLAY_DELAY_MS + 10);
      });
      expect(submit).not.toHaveBeenCalled();
      expect(screen.queryByTestId("secondary-autoplay-toast")).toBeNull();
      expect(dialog()).toBeInTheDocument();
    } finally {
      vi.useRealTimers();
    }
  });
});

describe("auto mode: decided in the background, the decision UI stays closed", () => {
  beforeEach(() => {
    localStorage.clear();
    localStorage.setItem(SECONDARY_PREP_MODE_KEY, "auto");
    vi.useFakeTimers();
  });
  afterEach(() => vi.useRealTimers());

  function prepareFollow(over: Partial<HarnessProps> = {}) {
    const ctx = setup({ history: hist(5), ...over });
    open();
    click("secondary-yes-btn");
    // Technology asks for the tech next; skip it for the plain follow plan: save.
    click("prep-save");
    return ctx;
  }
  const advance = (ms = AUTO_PLAY_DELAY_MS + 10) =>
    act(() => {
      vi.advanceTimersByTime(ms);
    });

  it("does not open the modal, shows the toast, sends once, and announces it", async () => {
    const { submit, again } = prepareFollow();
    again({ history: hist(6), choice: secondaryChoice("pok7technology") });
    expect(dialog()).toBeNull();
    expect(screen.queryByTestId("strategy-secondary-panel")).toBeNull();
    expect(screen.getByTestId("secondary-autoplay-toast").textContent).toMatch(/Follow Technology/);
    expect(screen.queryByTestId("secondary-prepared-bar")).toBeNull();
    await act(async () => {
      vi.advanceTimersByTime(AUTO_PLAY_DELAY_MS + 10);
    });
    expect(submit).toHaveBeenCalledTimes(1);
    expect(submit).toHaveBeenCalledWith("yes");
    // Still no UI while the answer is in flight, and the player is told afterwards.
    expect(dialog()).toBeNull();
    expect(screen.getByTestId("secondary-autoplayed-toast").textContent).toMatch(/Auto-played your prepared secondary/);
  });

  it("Cancel opens the real decision for a manual answer", () => {
    const { submit, again } = prepareFollow();
    again({ history: hist(6), choice: secondaryChoice("pok7technology") });
    expect(dialog()).toBeNull();
    click("secondary-autoplay-cancel");
    advance();
    expect(submit).not.toHaveBeenCalled();
    expect(dialog()).toBeInTheDocument();
    expect(screen.getByTestId("strategy-secondary-panel")).toBeInTheDocument();
  });

  it("opens the UI when the plan does not validate (Needs review)", () => {
    const { submit, again } = prepareFollow();
    again({ history: hist(6), choice: secondaryChoice("pok7technology", {}, [option("no")]) });
    expect(dialog()).toBeInTheDocument();
    expect(screen.getByTestId("secondary-needs-review")).toBeInTheDocument();
    advance();
    expect(submit).not.toHaveBeenCalled();
  });

  it("opens the UI at once when the submit is rejected", async () => {
    const { submit, again } = prepareFollow();
    submit.mockRejectedValueOnce(new Error("stale"));
    again({ history: hist(6), choice: secondaryChoice("pok7technology") });
    await act(async () => {
      vi.advanceTimersByTime(AUTO_PLAY_DELAY_MS + 10);
    });
    expect(submit).toHaveBeenCalledTimes(1);
    expect(dialog()).toBeInTheDocument();
  });

  it("opens the UI after the safety timeout if nothing moved on", async () => {
    const { again } = prepareFollow();
    again({ history: hist(6), choice: secondaryChoice("pok7technology") });
    await act(async () => {
      vi.advanceTimersByTime(AUTO_PLAY_DELAY_MS + 10);
    });
    expect(dialog()).toBeNull();
    act(() => {
      vi.advanceTimersByTime(AUTO_PLAY_HOLD_LIMIT_MS + 10);
    });
    expect(dialog()).toBeInTheDocument();
  });

  it("multi-step: the technology pick is answered without opening the UI, then it ends", async () => {
    const ctx = setup({ history: hist(5) });
    open();
    click("secondary-yes-btn");
    const tech = await pickFirstTech();
    ctx.again({ history: hist(6), choice: secondaryChoice("pok7technology", { nonce: "w" }) });
    await act(async () => {
      vi.advanceTimersByTime(AUTO_PLAY_DELAY_MS + 10);
    });
    expect(ctx.submit).toHaveBeenLastCalledWith("yes");
    const asked = [...(ctx.props.events ?? playedLog("Technology")), actionEvent("action_5", "b", { detail: "b followed" })];
    ctx.again({
      events: asked,
      history: hist(7),
      choice: stepChoice("research_technology", [option(tech, "research"), option("zz", "research")], "t1"),
    });
    expect(dialog()).toBeNull();
    await act(async () => {
      vi.advanceTimersByTime(AUTO_PLAY_DELAY_MS + 10);
    });
    expect(ctx.submit).toHaveBeenLastCalledWith(tech);
    // The next, unplanned question opens normally.
    ctx.again({ history: hist(8), choice: stepChoice("something_else", [option("x"), option("y")], "n3") });
    expect(dialog()).toBeInTheDocument();
  });

  it("an undo (history generation change) never arms it: the real UI opens", () => {
    const { submit, again } = prepareFollow({ history: hist(8) });
    again({ history: hist(6, 1, 2), choice: secondaryChoice("pok7technology") });
    advance();
    expect(submit).not.toHaveBeenCalled();
    expect(dialog()).toBeInTheDocument();
  });
});

describe("the setting in the player sheet", () => {
  beforeEach(() => localStorage.clear());
  const sheet = (userSeat?: string) =>
    render(
      <PlayerSheet
        players={players()}
        userSeat={userSeat}
        seatingOrder={["a", "b", "c"]}
        revealedObjectives={[]}
        board={boardWith([])}
        table={{ revealed_objectives: [], scored_objectives: {}, unclaimed_strategy_cards: [], strategy_card_goods: {}, laws: {} }}
      />,
    );

  it("defaults to Review, switches to Auto and persists", () => {
    sheet("b");
    const mode = screen.getByTestId("secondary-prep-mode");
    expect(mode.getAttribute("data-mode")).toBe("review");
    expect(screen.getByTestId("secondary-prep-mode-review").getAttribute("aria-pressed")).toBe("true");
    fireEvent.click(screen.getByTestId("secondary-prep-mode-auto"));
    expect(localStorage.getItem(SECONDARY_PREP_MODE_KEY)).toBe("auto");
    expect(screen.getByTestId("secondary-prep-mode").getAttribute("data-mode")).toBe("auto");
    fireEvent.click(screen.getByTestId("secondary-prep-mode-review"));
    expect(localStorage.getItem(SECONDARY_PREP_MODE_KEY)).toBe("review");
  });

  it("is not shown to spectators", () => {
    sheet(undefined);
    expect(screen.queryByTestId("secondary-prep-mode")).toBeNull();
  });
});
