import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type {
  BoardView,
  GameEvent,
  HistoryStatus,
  PendingChoiceDto,
  PlayerView,
  SecondaryBlocker,
} from "../protocol/types.ts";
import type { BasketPlan, SecondaryPreviewReply } from "../protocol/client.ts";
import { planetResourceMap } from "../presentation/PlanetResourcesContext.tsx";
import { TOKEN_POOLS, type TokenStep } from "../presentation/commandTokens.ts";
import type { CommandTokenDraftApi } from "../presentation/CommandTokenDraftContext.tsx";
import {
  detectStrategicAction,
  prepareEligibility,
} from "../presentation/strategicAction.ts";
import {
  isSecondaryQuestion,
  resolveStep,
  type SecondaryPlan,
  type StructureUnit,
  type StepResolution,
  type TokenPrefill,
  leadershipTokens,
} from "../presentation/secondaryPlan.ts";
import {
  buildDryChoice,
  nextDryStep,
  type DryChoice,
  type DryStep,
} from "../presentation/dryChoice.ts";
import { paymentPlanetKey } from "../presentation/paymentDraft.ts";
import { optionUnit, stepUnitKey } from "../presentation/productionDraft.ts";
import {
  checkLeadershipReply,
  leadershipScript,
  toExactResult,
  type ExactResult,
  type LeadershipCheck,
} from "../presentation/secondaryPreview.ts";
import { usePreparedPlan } from "./useSecondaryPlan.ts";
import { useSecondaryAutoPlay } from "./useSecondaryAutoPlay.ts";
import type { ReadyPlanetsDraftApi } from "../presentation/ReadyPlanetsDraftContext.tsx";

const NOTHING_SENT: ReadonlySet<string> = new Set();

/** A reply that tells us nothing: the estimate is used. */
const NO_EXACT: ExactResult = { kind: "none", why: "no preview available" };

/** How long a burst of new events waits before the preview is asked again while preparing. */
export const PREVIEW_REFRESH_DEBOUNCE_MS = 400;
/** How long to wait before asking again after the server said "too many previews". */
export const PREVIEW_RETRY_MS = 250;

export interface UseSecondaryPrepareInput {
  gameId?: string;
  viewerSeat?: string | null;
  players: readonly PlayerView[];
  events: readonly GameEvent[];
  board?: BoardView;
  phase?: string;
  history?: HistoryStatus;
  /** The engine's own pending decision (never the dry one). */
  realChoice: PendingChoiceDto | null;
  /** A pipeline or a history change is busy: nothing is sent on the player's behalf. */
  busy?: boolean;
  submitChoice: (optionId: string) => Promise<void>;
  submitBatch?: (plan: BasketPlan) => Promise<void>;
  /**
   * The server's read-only preview of the secondary in progress ("what would my seat be asked if
   * its window opened now"). Absent, or rejecting/refusing, the panel keeps its client-side
   * estimates exactly as before.
   */
  previewSecondary?: (
    card: string,
    primary: string,
    answers: readonly string[],
  ) => Promise<SecondaryPreviewReply>;
  /** Changes whenever the game moved (new events, a new history generation): refreshes the preview. */
  refreshKey?: string;
  /**
   * Sends planned builds the way the production builder's "Confirm builds" does: one batch for a
   * single build, its build queue for several. Provided by the shell that owns the builder.
   */
  submitProduction?: (
    units: string[],
    destination: string,
    real: PendingChoiceDto,
  ) => Promise<void>;
}

/** What the panel can say about the engine's exact preview for the step being shown. */
export interface ExactInfo {
  /** `exact`: the engine's own question is shown; `loading`: asked, no answer yet; `estimate`: client estimate. */
  status: "exact" | "loading" | "estimate";
  /** The engine says the secondary would not be offered to this seat as of now. */
  notAsked: SecondaryBlocker | null;
}

/**
 * Preparing a strategy-card secondary with the usual UI.
 *
 * While another seat resolves a card the viewer has not been asked about, the viewer can open
 * "preparation mode": the app then shows a stand-in (a dry choice) for each question of the card in
 * place of a pending decision, so the real components render it and the map highlights its
 * planets. Answering one calls `submitChoice`/`submitBatch` of THIS hook, which record the
 * semantic plan on the device instead of sending anything.
 *
 * The stand-in is the engine's own question when the server can preview it (technology list with
 * skips and the payment plan, structure sites, the home production build list ...); otherwise a
 * client-side estimate, flagged approximate. The preview is "as of now": it is refreshed while the
 * panel is open, and the plan is re-validated against the real question when it opens.
 *
 * When the real question opens the plan is validated against its options and either offered for
 * one click (review), played in the background (auto) or flagged "Needs review".
 */
export function useSecondaryPrepare({
  gameId,
  viewerSeat,
  players,
  events,
  board,
  phase,
  history,
  realChoice,
  busy,
  submitChoice,
  submitBatch,
  previewSecondary,
  refreshKey,
  submitProduction,
}: UseSecondaryPrepareInput) {
  const generation = history?.generation ?? 0;
  const action = useMemo(
    () => detectStrategicAction({ events, players, phase }),
    [events, players, phase],
  );
  // Did the event log only grow since the last commit? Distinguishes a committed batch (session
  // replaced, generation bumped) from an undo, redo or restore (log rewritten).
  const logSeen = useRef<{ length: number; lastId?: string }>({ length: 0 });
  const logExtended =
    logSeen.current.length === 0 ||
    (events.length >= logSeen.current.length &&
      events[logSeen.current.length - 1]?.id === logSeen.current.lastId);
  useEffect(() => {
    logSeen.current = { length: events.length, lastId: events[events.length - 1]?.id };
  });
  const eligibility = prepareEligibility(action, viewerSeat, players, events);
  const ready = events.length > 0 && players.length > 0;
  const { plan, set, clear } = usePreparedPlan({
    gameId,
    viewerSeat,
    actionKey: action?.key ?? null,
    generation,
    ready,
  });
  const viewer = players.find((player) => player.id === viewerSeat);

  const mine = Boolean(realChoice && viewerSeat && realChoice.actor === viewerSeat);
  const canPrepare = Boolean(action && eligibility.canPrepare && !mine && viewer);

  // ---- preparation mode: which dry question is showing -----------------------------------------
  const [step, setStep] = useState<DryStep | null>(null);
  const [firstPlanet, setFirstPlanet] = useState<string | null>(null);
  /** The answer given to the window question (`yes`, or a waiver's id), which scripts the previews after it. */
  const [follow, setFollow] = useState("yes");
  /** The option id of the first planned build, which scripts the payment preview. */
  const [firstBuild, setFirstBuild] = useState<string | null>(null);
  /** Leadership: what the engine said about the last purchase saved or refused (this device, this action). */
  const [leadershipCheck, setLeadershipCheck] = useState<LeadershipCheck | null>(null);
  const preparing = step !== null && canPrepare;
  // The mode ends by itself when its action ends, or the viewer's real question arrives.
  useEffect(() => {
    if (step !== null && !canPrepare) setStep(null);
  }, [step, canPrepare]);

  // ---- the engine's exact preview ---------------------------------------------------------------
  const [exactByKey, setExactByKey] = useState<Record<string, ExactResult>>({});
  const [asking, setAsking] = useState(0);
  const unsupported = useRef(false);
  const sequence = useRef(0);
  const applied = useRef(new Map<string, number>());
  const actionKey = action?.key ?? null;
  const exactKey = useCallback(
    (answers: readonly string[]) => `${actionKey ?? ""}|${answers.join(">")}`,
    [actionKey],
  );

  /** The answers that script the preview of `target` (the window question needs none). */
  const answersFor = useCallback(
    (target: DryStep, overrides: { follow?: string; planet?: string | null; build?: string | null } = {}) => {
      const followed = overrides.follow ?? follow;
      switch (target) {
        case "secondary":
          return [];
        case "planet2":
          return [followed, overrides.planet ?? firstPlanet ?? ""];
        case "pay":
          return [followed, overrides.build ?? firstBuild ?? ""];
        default:
          return [followed];
      }
    },
    [follow, firstPlanet, firstBuild],
  );

  /** Whether a preview can be asked at all (it cannot for a spectator or an older server). */
  const canAsk = useCallback(
    () => Boolean(action && previewSecondary && !unsupported.current),
    [action, previewSecondary],
  );

  /** Asks the server (or answers from the cache); never throws: an unusable reply is `none`. */
  const fetchExact = useCallback(
    async (answers: readonly string[], force = false): Promise<ExactResult> => {
      if (!action || !previewSecondary || unsupported.current) return NO_EXACT;
      if (answers.some((answer) => answer === "")) return NO_EXACT;
      const key = exactKey(answers);
      const cached = exactByKey[key];
      if (cached && !force) return cached;
      const mine = ++sequence.current;
      setAsking((count) => count + 1);
      try {
        let reply = await previewSecondary(action.card, action.primary, answers);
        // The server answers at most one preview at a time per connection and not more often than
        // every 200 ms (a click right after the panel's own refresh): that is "try again", never "none".
        for (let attempt = 0; attempt < 3 && reply.kind === "refused" && reply.reason === "rate_limited"; attempt++) {
          await new Promise((resolve) => window.setTimeout(resolve, PREVIEW_RETRY_MS));
          reply = await previewSecondary(action.card, action.primary, answers);
        }
        const result = toExactResult(reply);
        // A slower, older answer never overwrites a newer one.
        if ((applied.current.get(key) ?? 0) < mine) {
          applied.current.set(key, mine);
          setExactByKey((all) => ({ ...all, [key]: result }));
        }
        return result;
      } catch (error) {
        if ((error as Error)?.name === "PreviewUnsupportedError") unsupported.current = true;
        const none: ExactResult = { kind: "none", why: String((error as Error)?.message ?? error) };
        if ((applied.current.get(key) ?? 0) < mine) {
          applied.current.set(key, mine);
          setExactByKey((all) => ({ ...all, [key]: none }));
        }
        return none;
      } finally {
        setAsking((count) => count - 1);
      }
    },
    [action, previewSecondary, exactKey, exactByKey],
  );

  // Nothing is kept across actions or after the mode closes: the next opening asks again.
  useEffect(() => {
    if (!preparing) {
      setExactByKey({});
      applied.current.clear();
    }
  }, [preparing, actionKey]);

  const stepAnswers = step ? answersFor(step) : null;
  const stepKey = stepAnswers ? exactKey(stepAnswers) : null;
  // Ask when the panel opens or moves to another question ...
  useEffect(() => {
    if (!preparing || !stepAnswers || !stepKey) return;
    if (exactByKey[stepKey] === undefined) void fetchExact(stepAnswers);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [preparing, stepKey]);
  // ... and again, after a short pause, whenever the game moved while it is open.
  useEffect(() => {
    if (!preparing || !stepAnswers) return;
    const timer = window.setTimeout(() => void fetchExact(stepAnswers, true), PREVIEW_REFRESH_DEBOUNCE_MS);
    return () => window.clearTimeout(timer);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [refreshKey]);

  const exact = stepKey ? (exactByKey[stepKey] ?? null) : null;
  const dry: DryChoice | null = useMemo(() => {
    if (!preparing || !action || !viewer || !step) return null;
    return buildDryChoice({
      action,
      viewer,
      board,
      step,
      taken: step === "planet2" && firstPlanet ? [firstPlanet] : [],
      exact,
    });
  }, [preparing, action, viewer, board, step, firstPlanet, exact]);

  const exactInfo: ExactInfo = {
    status: dry?.exact ? "exact" : exact === null && asking > 0 ? "loading" : "estimate",
    notAsked: exact?.kind === "not_asked" ? exact.blocker : null,
  };

  const open = useCallback(() => {
    setFirstPlanet(null);
    setFirstBuild(null);
    setFollow("yes");
    setLeadershipCheck(null);
    setStep("secondary");
  }, []);
  /** Leaves preparation mode; the plan stays saved. */
  const close = useCallback(() => {
    setStep(null);
    setFirstPlanet(null);
    setFirstBuild(null);
  }, []);

  const base: SecondaryPlan | null = action ? (plan ?? { card: action.card, follow: true }) : null;

  /** The preparation-mode replacement for `onSubmitChoice`: records a plan, sends nothing. */
  const prepareSubmit = useCallback(
    async (optionId: string) => {
      if (!action || !step) return;
      const card = action.card;
      const finish = () => close();
      if (step === "secondary") {
        if (optionId === "no" || optionId === "decline") {
          set({ card, follow: false });
          return finish();
        }
        set({ card, follow: true });
        setFollow(optionId);
        const next = nextDryStep(action.family, "secondary", { planets: 0 });
        if (!next) return finish();
        // The next question may only exist as the engine's own (Warfare's build list): ask first.
        const nextExact = canAsk() ? await fetchExact(answersFor(next, { follow: optionId })) : null;
        const probe = viewer && buildDryChoice({ action, viewer, board, step: next, exact: nextExact });
        // A follow-up with nothing to pick (no exhausted planet, nothing researchable) ends the plan.
        if (!probe) return finish();
        return setStep(next);
      }
      const current = base ?? { card, follow: true };
      if (step === "tech") {
        if (optionId === "decline") {
          set({ ...current, follow: true, tech: undefined });
          return finish();
        }
        set({ ...current, follow: true, tech: optionId });
        return finish();
      }
      if (step === "planet1") {
        set({ ...current, follow: true, planets: [optionId] });
        setFirstPlanet(optionId);
        const nextExact = canAsk() ? await fetchExact(answersFor("planet2", { planet: optionId })) : null;
        const probe =
          viewer &&
          buildDryChoice({ action, viewer, board, step: "planet2", taken: [optionId], exact: nextExact });
        if (!probe) return finish();
        return setStep("planet2");
      }
      if (step === "planet2") {
        const first = current.planets?.[0];
        set({ ...current, follow: true, planets: first ? [first, optionId] : [optionId] });
        return finish();
      }
      if (step === "site") {
        if (optionId === "decline") {
          set({ ...current, follow: true, structure: undefined });
          return finish();
        }
        const [unit, , planet] = optionId.split("|");
        if ((unit === "pds" || unit === "spacedock") && planet) {
          set({ ...current, follow: true, structure: { unit: unit as StructureUnit, planet } });
        }
        return finish();
      }
      if (step === "produce") {
        // "Done producing" without a build: follow, build nothing.
        set({ ...current, follow: true, production: undefined });
        return finish();
      }
    },
    [action, step, base, set, close, viewer, board, fetchExact, answersFor, canAsk],
  );

  /**
   * The preparation-mode replacement for the staged batches: Leadership's token purchase, Warfare's
   * production builder ("Confirm builds") and the payment drawer for its first build.
   */
  const prepareBatch = useCallback(
    async (batch: BasketPlan) => {
      if (!action) return;
      if (batch.kind === "tokens") {
        const pools = { tactic: 0, fleet: 0, strategic: 0 };
        for (const tokenStep of batch.steps) {
          if (tokenStep.kind !== "pool") continue;
          const pool = TOKEN_POOLS.find((candidate) => tokenStep.pool === `${candidate}_tokens`);
          if (pool) pools[pool] += 1;
        }
        const total = pools.tactic + pools.fleet + pools.strategic;
        if (total === 0) {
          set({ card: action.card, follow: false });
          setLeadershipCheck(null);
          close();
          return;
        }
        // The payment chosen on the map, kept by meaning (planets by purchase-planet id).
        const payment = {
          planets: batch.steps.flatMap((entry) => (entry.kind === "exhaust" ? [entry.planet] : [])),
          tradeGoods: batch.steps.filter((entry) => entry.kind === "trade_good").length,
        };
        // The engine checks the whole purchase (every payment, pool and "again?") against its own flow.
        let check: LeadershipCheck = { kind: "unchecked", why: "the game could not be asked" };
        if (canAsk()) {
          check = checkLeadershipReply(await fetchExact(leadershipScript(follow, batch.steps), true));
        }
        if (check.kind === "rejected") {
          // Nothing is saved; the panel stays open on the same staging to be changed.
          setLeadershipCheck(check);
          const refusal = new Error(`The game would not accept this purchase as of now: ${check.reason}.`);
          refusal.name = "PurchaseRejected";
          throw refusal;
        }
        setLeadershipCheck(check);
        set({ card: action.card, follow: true, leadership: { pools, payment } });
        close();
        return;
      }
      const current = base ?? { card: action.card, follow: true };
      if (batch.kind === "ready_planets") {
        // Diplomacy: both planets were picked together; the plan keeps them by planet id.
        if (step !== "planet1") return;
        const planets = batch.steps.map((entry) => entry.planet);
        set({ ...current, follow: true, planets });
        close();
        return;
      }
      if (batch.kind === "production" && step === "produce") {
        const builds = batch.steps.flatMap((entry) => (entry.kind === "produce" ? [stepUnitKey(entry)] : []));
        if (!builds.length) {
          set({ ...current, follow: true, production: undefined });
          close();
          return;
        }
        set({ ...current, follow: true, production: { builds } });
        // The first build's payment, when the engine would ask for one.
        const offered = dry?.choice.options.find((option) => optionUnit(option) === builds[0]);
        if (offered) {
          setFirstBuild(offered.id);
          const nextExact = await fetchExact(answersFor("pay", { build: offered.id }));
          if (
            nextExact.kind === "question" &&
            nextExact.choice.context?.subtype === "pay_resources"
          ) {
            setStep("pay");
            return;
          }
        }
        close();
        return;
      }
      if (batch.kind === "payment" && step === "pay") {
        const planets = batch.steps.flatMap((entry) =>
          entry.kind === "exhaust" ? [paymentPlanetKey(entry.planet)] : [],
        );
        const tradeGoods = batch.steps.filter((entry) => entry.kind === "trade_good").length;
        if (current.production) {
          set({ ...current, follow: true, production: { ...current.production, payment: { planets, tradeGoods } } });
        }
        close();
      }
    },
    [action, step, base, dry, set, close, fetchExact, answersFor, canAsk, follow],
  );

  // ---- when the real question opens ------------------------------------------------------------
  // The follow-up prompts (technology, planets, site, production) belong to the plan only once its
  // own window opened in this action: a decision seen after a page load, or any other research
  // prompt, is not answered by it.
  const [openedFor, setOpenedFor] = useState<string | null>(null);
  /** Parts of the plan already sent in this action (production, payment): later offers are not theirs. */
  const [sent, setSent] = useState<{ key: string | null; done: ReadonlySet<string> }>({
    key: null,
    done: new Set(),
  });
  const done: ReadonlySet<string> = sent.key === actionKey ? sent.done : NOTHING_SENT;
  const secondaryOpen = Boolean(
    mine &&
      realChoice &&
      action &&
      isSecondaryQuestion(realChoice) &&
      realChoice.details?.card === action.card,
  );
  useEffect(() => {
    if (secondaryOpen && action) setOpenedFor(action.key);
  }, [secondaryOpen, action]);
  const applicable = secondaryOpen || (action !== null && openedFor === action.key);
  const planetResources = useMemo(() => planetResourceMap(board), [board]);
  const resolution: StepResolution = useMemo(
    () =>
      plan && applicable
        ? resolveStep(plan, realChoice, viewerSeat, done, planetResources)
        : { kind: "none" as const },
    [plan, applicable, realChoice, viewerSeat, done, planetResources],
  );

  /**
   * What the token panel is opened with: the saved purchase while preparing, and at arrival the
   * prepared purchase (or, when its payment no longer fits, the Auto-pay replacement to confirm).
   */
  const tokenPrefill: TokenPrefill | null = useMemo(() => {
    if (preparing) {
      const saved = plan?.leadership;
      if (!saved || !plan.follow) return null;
      const bought = leadershipTokens(saved.pools);
      const payment = saved.payment;
      return {
        bought,
        pools: saved.pools,
        override: payment ? { planetIds: payment.planets, tradeGoods: payment.tradeGoods } : null,
      };
    }
    if (resolution.kind === "tokens" || resolution.kind === "review") return resolution.prefill ?? null;
    return null;
  }, [preparing, plan, resolution]);

  /**
   * Diplomacy: the planets the ready bar is opened with. While preparing, the saved plan's; at
   * arrival the prepared planets (or those of them still exhausted, when the plan needs a review).
   */
  const readyPrefill: string[] | null = useMemo(() => {
    if (preparing) return step === "planet1" && plan?.follow && plan.planets?.length ? plan.planets : null;
    if (resolution.kind === "ready") return resolution.planetIds;
    if (resolution.kind === "review") return resolution.ready ?? null;
    return null;
  }, [preparing, step, plan, resolution]);

  const markSent = useCallback(
    (part: string, on: boolean) =>
      setSent((before) => {
        const kept = before.key === actionKey ? new Set(before.done) : new Set<string>();
        if (on) kept.add(part);
        else kept.delete(part);
        return { key: actionKey, done: kept };
      }),
    [actionKey],
  );
  /** Marks first (the next decision may arrive before the promise settles), undoes on failure. */
  const sendPart = useCallback(
    async (part: string, send: () => Promise<void>) => {
      markSent(part, true);
      try {
        await send();
      } catch (error) {
        markSent(part, false);
        throw error;
      }
    },
    [markSent],
  );

  const submitTokens = submitBatch
    ? (steps: TokenStep[]) => submitBatch({ kind: "tokens", steps })
    : undefined;
  const sendProduction =
    submitProduction && realChoice
      ? (units: string[], destination: string) =>
          sendPart("production", () => submitProduction(units, destination, realChoice))
      : undefined;
  const sendPayment = submitBatch
    ? (steps: Extract<StepResolution, { kind: "payment" }>["steps"]) =>
        sendPart("payment", () => submitBatch({ kind: "payment", steps }))
    : undefined;
  const sendReady = submitBatch
    ? (planetIds: string[]) =>
        submitBatch({
          kind: "ready_planets",
          steps: planetIds.map((planet) => ({ kind: "ready_planet" as const, planet })),
        })
    : undefined;
  const { pending, played, cancel, holding } = useSecondaryAutoPlay({
    choice: realChoice,
    viewerSeat,
    history,
    logExtended,
    busy,
    resolution,
    submitOption: submitChoice,
    submitTokens,
    submitProduction: sendProduction,
    submitPayment: sendPayment,
    submitReady: sendReady,
  });

  /** Sends a prepared answer for the viewer's real decision (the Review bar's Confirm). */
  const play = useCallback(
    async (target: StepResolution) => {
      if (target.kind === "option") await submitChoice(target.optionId);
      else if (target.kind === "tokens") await submitTokens?.(target.steps);
      else if (target.kind === "production") await sendProduction?.(target.units, target.destination);
      else if (target.kind === "payment") await sendPayment?.(target.steps);
      else if (target.kind === "ready") await sendReady?.(target.planetIds);
    },
    [submitChoice, submitTokens, sendProduction, sendPayment, sendReady],
  );

  /** What the app shows as the pending decision: the dry one while preparing, nothing while held. */
  const shownChoice = dry ? dry.choice : holding ? null : realChoice;

  return {
    action,
    canPrepare,
    plan,
    clear,
    preparing,
    open,
    close,
    dry,
    exactInfo,
    leadershipCheck,
    tokenPrefill,
    readyPrefill,
    shownChoice,
    prepareSubmit,
    prepareBatch,
    realChoice,
    mine,
    resolution,
    play,
    pending,
    played,
    cancel,
    holding,
  };
}

export type SecondaryPrepare = ReturnType<typeof useSecondaryPrepare>;

/**
 * Leadership: opens the usual token panel with the prepared purchase already staged (tokens, their
 * pools and the payment on the map). Once per question, so the player's own changes stay.
 */
export function useTokenPrefill(
  prep: Pick<SecondaryPrepare, "tokenPrefill" | "holding">,
  nonce: string | null | undefined,
  tokenDraft: Pick<CommandTokenDraftApi, "update">,
): void {
  const prefilledFor = useRef<string | null>(null);
  const { tokenPrefill, holding } = prep;
  const key = nonce ?? null;
  const { update } = tokenDraft;
  useEffect(() => {
    if (prefilledFor.current !== key) prefilledFor.current = null;
    if (!tokenPrefill || !key || holding) return;
    if (prefilledFor.current === key) return;
    prefilledFor.current = key;
    update((draft) => ({
      ...draft,
      staging: { ...tokenPrefill.pools },
      bought: tokenPrefill.bought,
      override: tokenPrefill.override,
    }));
  }, [tokenPrefill, key, holding, update]);
}

/**
 * Diplomacy: the ready bar opens with the prepared planets already picked (Confirm stays the
 * player's). Once per question, so the player's own changes stay.
 */
export function useReadyPrefill(
  prep: Pick<SecondaryPrepare, "readyPrefill" | "holding">,
  nonce: string | null | undefined,
  readyDraft: Pick<ReadyPlanetsDraftApi, "set" | "view">,
): void {
  const prefilledFor = useRef<string | null>(null);
  const { readyPrefill, holding } = prep;
  const key = nonce ?? null;
  const { set, view } = readyDraft;
  useEffect(() => {
    if (prefilledFor.current !== key) prefilledFor.current = null;
    if (!readyPrefill || !key || holding || !view) return;
    if (prefilledFor.current === key) return;
    prefilledFor.current = key;
    set(readyPrefill);
  }, [readyPrefill, key, holding, set, view]);
}
