import { useParticipantText } from "../presentation/PlayerIdentity.tsx";
import React, { useState, useEffect, useMemo } from "react";
import { usePlanetInfluence, usePlanetResources } from "../presentation/PlanetResourcesContext.tsx";
import { PendingChoiceDto, PlayerView } from "../protocol/types.ts";
import { ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { usePipelineRunner, SemanticIntent } from "../hooks/usePipelineRunner.ts";
import { WorkflowShell } from "./WorkflowShell.tsx";
import { useProductionPaymentPanel } from "../presentation/ProductionPaymentContext.tsx";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { DecisionHeader } from "./DecisionHeader.tsx";
import { PlanetValue, ValueUnit, valueKind } from "./PlanetValueIcons.tsx";
import { productionTotalText } from "../presentation/productionTotalText.ts";
import {
  buildPaymentSteps,
  derivePaymentOffer,
  paymentPlanetKey,
  paymentProblem,
  summarizePayment,
  suggestAutoPay,
  togglePlanetInDraft,
} from "../presentation/paymentDraft.ts";
import {
  usePaymentDraftState,
  useSharedPaymentDraft,
} from "../presentation/PaymentDraftContext.tsx";

export interface PaymentDrawerProps {
  choice: PendingChoiceDto | null;
  model?: ChoiceRendererModel | null;
  viewerSeat?: string | null;
  player?: PlayerView | null;
  onSubmit: (optionId: string) => Promise<void>;
  onSubmitBatch?: (plan: import("../protocol/client.ts").BasketPlan) => Promise<void>;
  isOpen: boolean;
  onClose: () => void;
  lastError?: string | null;
  selectedOptionId?: string;
}

interface DraftPlanet {
  id: string;
  planetName: string;
  worth: number;
  label: string;
  sourceKind?: string;
}

export const PaymentDrawer: React.FC<PaymentDrawerProps> = ({
  choice,
  model,
  viewerSeat,
  player,
  onSubmit,
  onSubmitBatch,
  isOpen,
  onClose,
  lastError,
  selectedOptionId,
}) => {
  const display = usePlayerIdentity();
  const present = useParticipantText();
  const shared = useSharedPaymentDraft();
  const resourcesOf = usePlanetResources();
  const influenceOf = usePlanetInfluence();
  const local = usePaymentDraftState(choice?.nonce);
  const { draft, togglePlanet, setTradeGoods, setDraft, reset } = shared ?? local;
  const selectedPlanetIds = draft.planetIds;
  const tradeGoodsToSpend = draft.tradeGoods;
  const [batchError, setBatchError] = useState<string | null>(null);
  const [batchRunning, setBatchRunning] = useState(false);

  const {
    executePipeline,
    isRunning: isPipelineRunning,
    lastError: pipelineError,
  } = usePipelineRunner(choice, onSubmit);

  const baseOffer = useMemo(
    () => (choice ? derivePaymentOffer(choice, model, resourcesOf) : null),
    [choice, model, resourcesOf],
  );
  const baseTradeGoods = player?.trade_goods ?? (baseOffer?.hasTradeGoodOption ? 1 : 0);
  // Staged builds are paid for once: the first question's panel is the whole production's bill.
  const production = useProductionPaymentPanel(choice, baseOffer, baseTradeGoods, draft, setDraft);
  const offer = baseOffer ? production.offer : null;
  const constraints = model?.outstanding?.[0] ?? choice?.context?.outstanding?.[0];
  const totalAmount = offer?.totalAmount ?? 0;
  const alreadyPaid = offer?.alreadyPaid ?? 0;
  const owed = offer?.owed ?? 0;
  const currency = offer?.currency ?? "Resources";
  const kind = valueKind(currency);
  const availablePlanets: DraftPlanet[] = offer?.planets ?? [];
  const hasTradeGoodOption = offer?.hasTradeGoodOption ?? false;
  const tradeGoodWorth = offer?.tradeGoodWorth ?? 1;

  useEffect(() => {
    setBatchError(null);
  }, [choice?.nonce]);

  // Without the shared draft (no map), a pick made elsewhere arrives as the selected option.
  useEffect(() => {
    if (
      !shared &&
      selectedOptionId &&
      availablePlanets.some((planet) => planet.id === selectedOptionId)
    ) {
      setDraft({
        ...draft,
        planetIds: draft.planetIds.includes(selectedOptionId)
          ? draft.planetIds
          : togglePlanetInDraft(draft.planetIds, selectedOptionId),
      });
    }
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [selectedOptionId, offer]);

  const maxTradeGoodsAvailable = player?.trade_goods ?? (hasTradeGoodOption ? 1 : 0);
  const summary = offer
    ? summarizePayment(offer, draft)
    : { committed: 0, shortfall: 0, surplus: 0, settled: false, fromPlanets: 0, fromTradeGoods: 0 };
  const totalCommitted = summary.committed;
  const credit = summary.surplus;
  const shortfall = summary.shortfall;
  const isSettled = summary.settled;
  const problem = offer ? paymentProblem(offer, draft, maxTradeGoodsAvailable) : null;
  const handleTogglePlanet = togglePlanet;
  const handleAutoPay = () => {
    if (!offer) return;
    const suggestion = suggestAutoPay(offer, maxTradeGoodsAvailable);
    setDraft({ planetIds: suggestion.planetIds, tradeGoods: suggestion.tradeGoods });
  };

  const handleConfirmPayment = async (
    isDirectSubmitting: boolean,
    submitDirect: (optionId: string) => Promise<void>,
  ) => {
    if (!isSettled || isPipelineRunning || isDirectSubmitting || batchRunning || !choice) return;

    // The one payment for a whole production: this question takes its part, the rest is kept.
    const part = production.split(draft);
    if (!part) {
      setBatchError("The staged payment does not fit this question. Reset and stage again.");
      return;
    }

    // Build execution list
    const intents: SemanticIntent[] = [];

    // 1. Planets to exhaust
    for (const planetId of part.planetIds) {
      intents.push({
        predicate: (opt) => opt.id === planetId,
      });
    }

    // 2. Trade goods to spend
    for (let i = 0; i < part.tradeGoods; i++) {
      intents.push({
        predicate: (opt) => opt.id === "trade_good",
      });
    }

    if (onSubmitBatch && intents.length > 0) {
      setBatchRunning(true);
      setBatchError(null);
      try {
        await onSubmitBatch({
          kind: "payment",
          steps: buildPaymentSteps(choice, baseOffer!, part),
        });
      } catch (error) {
        if (production.panel) production.revert();
        setBatchError(error instanceof Error ? error.message : String(error));
      } finally {
        setBatchRunning(false);
      }
      return;
    }

    if (intents.length === 1) {
      const targetOption = choice.options.find(intents[0].predicate);
      if (targetOption) await submitDirect(targetOption.id);
    } else if (intents.length > 1) {
      executePipeline(intents);
    }
  };

  if (!choice) return null;

  return (
    isOpen && (
      <section
        role="region"
        aria-label="Payment and Economy"
        data-testid="payment-drawer"
        className="payment-drawer panel"
      >
        {/* Header */}
        <DecisionHeader
          actor={choice.actor}
          choice={choice}
          title={
            <>
              {production.panel ? `Pay for ${production.panel.builds} units: total ` : "Pay "}
              <PlanetValue kind={kind} value={owed} size="bar" state="ready" />
            </>
          }
          instruction={
            production.panel
              ? `You pay once: the engine asks for ${production.panel.builds} builds one after another and the later payments follow this choice.`
              : present(choice.prompt)
          }
          progress={
            constraints?.amount === undefined
              ? undefined
              : (
                  <>
                    {alreadyPaid} / {totalAmount} <ValueUnit kind={kind} /> already paid
                  </>
                )
          }
          onMinimize={onClose}
          titleTestId="payment-drawer-title"
          minimizeTestId="close-payment-drawer"
        />

        <WorkflowShell
          choice={choice}
          model={model}
          viewerSeat={viewerSeat}
          onSubmit={onSubmit}
          lastError={batchError ?? lastError}
          spectatorNotice={`Observing payment in progress for ${display(choice.actor).label}...`}
          spectatorNoticeTestId="spectator-payment-notice"
          errorTestId="payment-error-banner"
        >
          {({ isActor, isDirectSubmitting, declineOption, submitDirect }) =>
            isActor && (
              <>
                {/* Progress and Debt Tally */}
                <div
                  data-testid="payment-tally-card"
                  className="workflow-card payment-drawer__tally"
                >
                  {production.panel && (
                    <strong className="production-total__sum" data-testid="payment-total-line">
                      {productionTotalText(production.panel.total, production.panel.builds)}
                    </strong>
                  )}
                  <div className="workflow-card--row">
                    <span className="text-muted">Total Owed:</span>
                    <PlanetValue kind={kind} value={owed} state="ready" />
                  </div>
                  <div className="workflow-card--row">
                    <span className="text-muted">Staged (not yet paid):</span>
                    <span
                      data-testid="committed-amount"
                      className={isSettled ? "text-success" : "text-accent"}
                    >
                      <PlanetValue
                        kind={kind}
                        value={totalCommitted}
                        state={totalCommitted === 0 ? "muted" : "ready"}
                      />
                    </span>
                  </div>
                  {credit > 0 && (
                    <div className="workflow-card--row text-warning">
                      <span>Potential overpayment (server determines credit):</span>
                      <PlanetValue kind={kind} value={credit} sign="+" state="ready" />
                    </div>
                  )}

                  {/* Progress bar */}
                  <progress
                    className="payment-drawer__progress"
                    data-settled={isSettled}
                    max={Math.max(owed, 1)}
                    value={Math.min(totalCommitted, owed)}
                  />
                </div>

                {/* Ready Planet Cards */}
                <div className="payment-drawer__list">
                  <div className="choice-workflow-eyebrow text-muted">
                    Ready Planets ({availablePlanets.length})
                  </div>

                  {availablePlanets.length === 0 ? (
                    <div className="text-faint">No offered planets available to exhaust.</div>
                  ) : (
                    availablePlanets.map((planet) => {
                      const isSelected = selectedPlanetIds.includes(planet.id);
                      return (
                        <label
                          key={planet.id}
                          data-testid={`planet-card-${planet.id}`}
                          className={`card payment-drawer__planet${isSelected ? " card--selected" : ""}`}
                          data-selected={isSelected}
                        >
                          <div className="workflow-row">
                            <input
                              type="checkbox"
                              checked={isSelected}
                              onChange={() => handleTogglePlanet(planet.id)}
                              disabled={isPipelineRunning || isDirectSubmitting}
                            />
                            <div>
                              <div
                                className="payment-drawer__planet-name"
                                data-selected={isSelected}
                              >
                                {planet.planetName}
                              </div>
                              {planet.sourceKind &&
                                planet.sourceKind !== currency.toLowerCase() && (
                                  <div className="text-warning">via {planet.sourceKind}</div>
                                )}
                            </div>
                          </div>
                          <span className="workflow-badge" data-testid={`planet-card-${planet.id}-values`}>
                            {/* Both values sit beside the planet; the one being paid with is marked. */}
                            {resourcesOf.has(paymentPlanetKey(planet.id)) || influenceOf.has(paymentPlanetKey(planet.id)) ? (
                              <>
                                <PlanetValue
                                  kind="resources"
                                  value={resourcesOf.get(paymentPlanetKey(planet.id)) ?? 0}
                                  state={kind === "resources" ? "ready" : "muted"}
                                />{" "}
                                <PlanetValue
                                  kind="influence"
                                  value={influenceOf.get(paymentPlanetKey(planet.id)) ?? 0}
                                  state={kind === "influence" ? "ready" : "muted"}
                                />
                              </>
                            ) : planet.worth ? (
                              <PlanetValue kind={kind} value={planet.worth} sign="+" />
                            ) : (
                              "Value unknown"
                            )}
                          </span>
                        </label>
                      );
                    })
                  )}

                  {/* Trade Goods Stepper */}
                  {hasTradeGoodOption && (
                    <div
                      data-testid="trade-goods-stepper"
                      className="workflow-card workflow-card--row"
                    >
                      <div>
                        <div>Trade Goods</div>
                        <div className="text-muted">
                          1 TG = <PlanetValue kind={kind} value={tradeGoodWorth} /> (Available:{" "}
                          {maxTradeGoodsAvailable})
                        </div>
                      </div>

                      <div className="workflow-row">
                        <button
                          type="button"
                          data-testid="tg-decrement-btn"
                          onClick={() => setTradeGoods((prev) => Math.max(0, prev - 1))}
                          disabled={
                            tradeGoodsToSpend <= 0 || isPipelineRunning || isDirectSubmitting
                          }
                          className="button button--secondary button--icon choice-workflow-close"
                        >
                          -
                        </button>
                        <span data-testid="tg-count" className="workflow-count">
                          {tradeGoodsToSpend}
                        </span>
                        <button
                          type="button"
                          data-testid="tg-increment-btn"
                          onClick={() =>
                            setTradeGoods((prev) => Math.min(maxTradeGoodsAvailable, prev + 1))
                          }
                          disabled={
                            tradeGoodsToSpend >= maxTradeGoodsAvailable ||
                            isPipelineRunning ||
                            isDirectSubmitting
                          }
                          className="button button--secondary button--icon choice-workflow-close"
                        >
                          +
                        </button>
                      </div>
                    </div>
                  )}
                  {hasTradeGoodOption && !player && (
                    <p className="text-muted">
                      Trade-good balance unavailable; only the currently offered spend is verified.
                    </p>
                  )}
                </div>

                {pipelineError && (
                  <div role="alert" className="workflow-error">
                    {pipelineError}
                  </div>
                )}

                {!production.panel && selectedPlanetIds.length + tradeGoodsToSpend > 1 && (
                  <p className="text-muted">
                    Pay submits one decision at a time. Later spends are unverified until the next
                    authoritative offer; the sequence stops if it changes or is rejected.
                  </p>
                )}

                {production.note && (
                  <p role="status" data-testid="production-pay-note" className="payment-drawer__problem text-warning">
                    {production.note}
                  </p>
                )}

                {problem && (
                  <p
                    role="status"
                    data-testid="payment-problem"
                    className="payment-drawer__problem text-warning"
                  >
                    {problem}
                  </p>
                )}

                {/* Action Footer */}
                <div className="payment-drawer__footer">
                  <button
                    type="button"
                    data-testid="auto-pay-btn"
                    className="button button--secondary"
                    onClick={handleAutoPay}
                    disabled={
                      isPipelineRunning ||
                      isDirectSubmitting ||
                      (availablePlanets.length === 0 && !hasTradeGoodOption)
                    }
                    title="Stage the planets that cover the bill with the least waste; nothing is paid until you confirm"
                  >
                    Auto-pay
                  </button>
                  <button
                    type="button"
                    data-testid="pick-on-map-btn"
                    className="button button--secondary"
                    onClick={onClose}
                    title="Close this list to pick planets on the map"
                  >
                    Pick on map
                  </button>
                  {(selectedPlanetIds.length > 0 || tradeGoodsToSpend > 0) && (
                    <button
                      type="button"
                      className="button button--secondary"
                      onClick={() => {
                        reset();
                      }}
                    >
                      Reset selection
                    </button>
                  )}
                  {production.panel && (
                    <button
                      type="button"
                      data-testid="ask-each-payment-btn"
                      className="button button--secondary"
                      disabled={isPipelineRunning || isDirectSubmitting}
                      title="Pay this build only; the next builds ask again"
                      onClick={() => {
                        production.askEach();
                        reset();
                      }}
                    >
                      Ask for each build
                    </button>
                  )}
                  {declineOption && (
                    <button
                      type="button"
                      data-testid="decline-payment-btn"
                      onClick={() => {
                        void submitDirect(declineOption.id);
                      }}
                      disabled={isPipelineRunning || isDirectSubmitting}
                      className="button button--secondary"
                    >
                      {declineOption.label || "Decline"}
                    </button>
                  )}
                  <button
                    type="button"
                    data-testid="confirm-payment-btn"
                    onClick={() => handleConfirmPayment(isDirectSubmitting, submitDirect)}
                    disabled={!isSettled || isPipelineRunning || isDirectSubmitting}
                    className="button button--primary"
                    data-ready={isSettled && !isPipelineRunning && !isDirectSubmitting}
                  >
                    {isPipelineRunning || isDirectSubmitting
                      ? "Paying..."
                      : shortfall > 0
                        ? `Stage ${shortfall} more to pay`
                        : `Pay (${totalCommitted} staged)`}
                  </button>
                </div>
              </>
            )
          }
        </WorkflowShell>
      </section>
    )
  );
};
