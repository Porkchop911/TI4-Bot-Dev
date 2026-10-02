import React, { useState, useEffect, useMemo } from "react";
import { PendingChoiceDto, PlayerView } from "../protocol/types.ts";
import { getPaymentPayload, ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { usePipelineRunner, SemanticIntent } from "../hooks/usePipelineRunner.ts";
import { WorkflowShell } from "./WorkflowShell.tsx";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { DecisionHeader } from "./DecisionHeader.tsx";

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
  const [selectedPlanetIds, setSelectedPlanetIds] = useState<string[]>([]);
  const [tradeGoodsToSpend, setTradeGoodsToSpend] = useState<number>(0);
  const [batchError, setBatchError] = useState<string | null>(null);
  const [batchRunning, setBatchRunning] = useState(false);

  const {
    executePipeline,
    isRunning: isPipelineRunning,
    lastError: pipelineError,
  } = usePipelineRunner(choice, onSubmit);

  const constraints = model?.outstanding?.[0] ?? choice?.context?.outstanding?.[0];
  const totalAmount =
    model?.selectionMode.mode === "quantity"
      ? model.selectionMode.target
      : (constraints?.amount ?? 0);
  const alreadyPaid =
    model?.selectionMode.mode === "quantity" ? model.selectionMode.paid : (constraints?.paid ?? 0);
  const owed = Math.max(0, totalAmount - alreadyPaid);
  const currency =
    model?.selectionMode.mode === "quantity"
      ? model.selectionMode.unit === "influence"
        ? "Influence"
        : "Resources"
      : choice?.context?.subtype === "pay_influence" ||
          constraints?.kind?.toLowerCase() === "influence"
        ? "Influence"
        : "Resources";

  // Extract payment options from legal options
  const { availablePlanets, hasTradeGoodOption, tradeGoodWorth } = useMemo(() => {
    if (!choice) {
      return {
        availablePlanets: [] as DraftPlanet[],
        hasTradeGoodOption: false,
        tradeGoodWorth: 1,
      };
    }

    const planets: DraftPlanet[] = [];
    let hasTG = false;
    let tgWorth = 1;

    for (const opt of choice.options) {
      if (opt.id === "decline" || opt.kind === "decline") {
        continue;
      }
      if (opt.id === "trade_good") {
        hasTG = true;
        const p = getPaymentPayload(opt);
        if (p.worth > 0) tgWorth = p.worth;
        continue;
      }
      if (opt.id.startsWith("exhaust|") || opt.kind === "pay") {
        const p = getPaymentPayload(opt);
        planets.push({
          id: opt.id,
          planetName: p.planetName || opt.label || "Planet",
          worth: p.worth > 0 ? p.worth : 0,
          label: opt.label,
          sourceKind: p.source,
        });
      }
    }

    return {
      availablePlanets: planets,
      hasTradeGoodOption: hasTG,
      tradeGoodWorth: tgWorth,
    };
  }, [choice, model]);

  // Reset draft state on new choice nonce
  useEffect(() => {
    setSelectedPlanetIds([]);
    setTradeGoodsToSpend(0);
    setBatchError(null);
  }, [choice?.nonce]);

  useEffect(() => {
    if (selectedOptionId && availablePlanets.some((planet) => planet.id === selectedOptionId)) {
      setSelectedPlanetIds((ids) =>
        ids.includes(selectedOptionId) ? ids : [...ids, selectedOptionId],
      );
    }
  }, [selectedOptionId, availablePlanets]);

  const maxTradeGoodsAvailable = player?.trade_goods ?? (hasTradeGoodOption ? 1 : 0);

  const committedFromPlanets = selectedPlanetIds.reduce((sum, id) => {
    const planet = availablePlanets.find((p) => p.id === id);
    return sum + (planet?.worth ?? 0);
  }, 0);

  const committedFromTG = tradeGoodsToSpend * tradeGoodWorth;
  const totalCommitted = committedFromPlanets + committedFromTG;
  const credit = Math.max(0, totalCommitted - owed);
  const isSettled = (totalCommitted > 0 || selectedPlanetIds.length > 0) && owed > 0;

  const handleTogglePlanet = (id: string) => {
    setSelectedPlanetIds((prev) =>
      prev.includes(id) ? prev.filter((p) => p !== id) : [...prev, id],
    );
  };

  const handleConfirmPayment = async (
    isDirectSubmitting: boolean,
    submitDirect: (optionId: string) => Promise<void>,
  ) => {
    if (!isSettled || isPipelineRunning || isDirectSubmitting || batchRunning || !choice) return;

    // Build execution list
    const intents: SemanticIntent[] = [];

    // 1. Planets to exhaust
    for (const planetId of selectedPlanetIds) {
      intents.push({
        predicate: (opt) => opt.id === planetId,
      });
    }

    // 2. Trade goods to spend
    for (let i = 0; i < tradeGoodsToSpend; i++) {
      intents.push({
        predicate: (opt) => opt.id === "trade_good",
      });
    }

    if (onSubmitBatch && intents.length > 0) {
      setBatchRunning(true);
      setBatchError(null);
      try {
        // When only two payment options remain and the first does not settle the
        // bill, the engine spends the sole remaining option without offering a
        // second choice. Do not include that automatic spend in the batch plan.
        const autoSpendsLast =
          choice.options.filter((option) => option.kind !== "decline").length === 2 &&
          selectedPlanetIds.length === 2 &&
          tradeGoodsToSpend === 0 &&
          (availablePlanets.find((planet) => planet.id === selectedPlanetIds[0])?.worth ?? 0) <
            owed;
        let remaining = owed;
        const planetsToSubmit = (
          autoSpendsLast ? selectedPlanetIds.slice(0, 1) : selectedPlanetIds
        ).filter((id) => {
          if (remaining <= 0) return false;
          remaining -= availablePlanets.find((planet) => planet.id === id)?.worth ?? 0;
          return true;
        });
        await onSubmitBatch({
          kind: "payment",
          steps: [
            ...planetsToSubmit.map((id) => ({
              kind: "exhaust" as const,
              planet: id.replace(/^exhaust\|/, ""),
            })),
            ...Array.from(
              {
                length: Math.min(
                  tradeGoodsToSpend,
                  Math.ceil(Math.max(0, remaining) / tradeGoodWorth),
                ),
              },
              () => ({ kind: "trade_good" as const }),
            ),
          ],
        });
      } catch (error) {
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
          title={`Pay ${owed} ${currency}`}
          instruction={choice.prompt}
          progress={
            constraints?.amount === undefined
              ? undefined
              : `${alreadyPaid} / ${totalAmount} ${currency} already paid`
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
                  <div className="workflow-card--row">
                    <span className="text-muted">Total Owed:</span>
                    <span>
                      {owed} {currency}
                    </span>
                  </div>
                  <div className="workflow-card--row">
                    <span className="text-muted">Staged (not yet paid):</span>
                    <span
                      data-testid="committed-amount"
                      className={isSettled ? "text-success" : "text-accent"}
                    >
                      {totalCommitted} {currency}
                    </span>
                  </div>
                  {credit > 0 && (
                    <div className="workflow-card--row text-warning">
                      <span>Potential overpayment (server determines credit):</span>
                      <span>
                        +{credit} {currency}
                      </span>
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
                          <span className="workflow-badge">
                            {planet.worth
                              ? `+${planet.worth} ${currency.slice(0, 3)}`
                              : "Value unknown"}
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
                          1 TG = {tradeGoodWorth} {currency.slice(0, 3)} (Available:{" "}
                          {maxTradeGoodsAvailable})
                        </div>
                      </div>

                      <div className="workflow-row">
                        <button
                          type="button"
                          data-testid="tg-decrement-btn"
                          onClick={() => setTradeGoodsToSpend((prev) => Math.max(0, prev - 1))}
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
                            setTradeGoodsToSpend((prev) =>
                              Math.min(maxTradeGoodsAvailable, prev + 1),
                            )
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

                {selectedPlanetIds.length + tradeGoodsToSpend > 1 && (
                  <p className="text-muted">
                    Pay submits one decision at a time. Later spends are unverified until the next
                    authoritative offer; the sequence stops if it changes or is rejected.
                  </p>
                )}

                {/* Action Footer */}
                <div className="payment-drawer__footer">
                  {(selectedPlanetIds.length > 0 || tradeGoodsToSpend > 0) && (
                    <button
                      type="button"
                      className="button button--secondary"
                      onClick={() => {
                        setSelectedPlanetIds([]);
                        setTradeGoodsToSpend(0);
                      }}
                    >
                      Reset selection
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
