import React, { useMemo, useState } from "react";
import { usePlanetResources } from "../presentation/PlanetResourcesContext.tsx";
import { PendingChoiceDto, PlayerView } from "../protocol/types.ts";
import type { BasketPlan } from "../protocol/client.ts";
import { ChoiceRendererModel, isDeclineOption } from "../presentation/choiceModel.ts";
import {
  buildPaymentSteps,
  derivePaymentOffer,
  paymentProblem,
  summarizePayment,
  suggestAutoPay,
} from "../presentation/paymentDraft.ts";
import { usePaymentDraftState, useSharedPaymentDraft } from "../presentation/PaymentDraftContext.tsx";
import { useProductionPaymentPanel } from "../presentation/ProductionPaymentContext.tsx";
import { usePipelineRunner } from "../hooks/usePipelineRunner.ts";
import { useParticipantText } from "../presentation/PlayerIdentity.tsx";
import { PlanetValue, ValueText, ValueUnit, valueKind } from "./PlanetValueIcons.tsx";

export interface PaymentBarProps {
  choice: PendingChoiceDto;
  model?: ChoiceRendererModel | null;
  viewerSeat?: string | null;
  player?: PlayerView | null;
  onSubmit: (optionId: string) => Promise<void>;
  onSubmitBatch?: (plan: BasketPlan) => Promise<void>;
  /** Reopens the full payment list. */
  onOpenList: () => void;
  lastError?: string | null;
}

/**
 * The payment confirm bar shown while the payment list is minimised: the payable planets on the
 * map are the controls, this bar shows paid against owed and confirms. It shares its staged
 * payment with the list and the map.
 */
export const PaymentBar: React.FC<PaymentBarProps> = ({
  choice,
  model,
  viewerSeat,
  player,
  onSubmit,
  onSubmitBatch,
  onOpenList,
  lastError,
}) => {
  const present = useParticipantText();
  const shared = useSharedPaymentDraft();
  const resourcesOf = usePlanetResources();
  const local = usePaymentDraftState(choice.nonce);
  const { draft, setDraft, reset } = shared ?? local;
  const [running, setRunning] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const { executePipeline, isRunning: pipelineRunning, lastError: pipelineError } = usePipelineRunner(
    choice,
    onSubmit,
  );

  const baseOffer = useMemo(() => derivePaymentOffer(choice, model, resourcesOf), [choice, model, resourcesOf]);
  const tradeGoodsAvailable = player?.trade_goods ?? (baseOffer.hasTradeGoodOption ? 1 : 0);
  const production = useProductionPaymentPanel(choice, baseOffer, tradeGoodsAvailable, draft, setDraft);
  const offer = production.offer;
  const isActor = Boolean(viewerSeat && choice.actor === viewerSeat);
  if (!isActor) return null;

  const summary = summarizePayment(offer, draft);
  const problem = paymentProblem(offer, draft, tradeGoodsAvailable);
  const declineOption = choice.options.find(isDeclineOption) ?? null;
  const busy = running || pipelineRunning;
  const kind = valueKind(offer.currency);
  const staged = draft.planetIds.length + draft.tradeGoods;

  const confirm = async () => {
    if (!summary.settled || busy) return;
    setError(null);
    // The one payment for a whole production: this question takes its part, the rest is kept.
    const part = production.split(draft);
    if (!part) {
      setError("The staged payment does not fit this question. Reset and stage again.");
      return;
    }
    const steps = buildPaymentSteps(choice, baseOffer, part);
    if (onSubmitBatch && steps.length > 0) {
      setRunning(true);
      try {
        await onSubmitBatch({ kind: "payment", steps });
      } catch (err) {
        if (production.panel) production.revert();
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setRunning(false);
      }
      return;
    }
    const ids = [
      ...part.planetIds,
      ...Array.from({ length: part.tradeGoods }, () => "trade_good"),
    ];
    if (ids.length === 1) {
      setRunning(true);
      try {
        await onSubmit(ids[0]);
      } catch (err) {
        if (production.panel) production.revert();
        setError(err instanceof Error ? err.message : String(err));
      } finally {
        setRunning(false);
      }
    } else {
      executePipeline(ids.map((id) => ({ predicate: (opt) => opt.id === id })));
    }
  };

  const errorMessage = error || pipelineError || lastError;

  return (
    <aside
      data-testid="payment-bar"
      className="choice-banner panel system-activation-bar payment-bar"
      aria-label="Payment"
    >
      <div className="system-activation-bar__body">
        <div className="system-activation-bar__prompt-row">
          <span className="badge badge--primary" data-testid="payment-bar-owed">
            {production.panel ? `Pay for ${production.panel.builds} units: total ` : "Pay "}
            <PlanetValue kind={kind} value={offer.owed} size="bar" state="ready" />
          </span>
          {!/\bpay\b/i.test(choice.prompt) && (
            <span className="system-activation-bar__prompt">{present(choice.prompt)}</span>
          )}
          <span className="system-activation-bar__hint text-muted">
            (Click highlighted planets on the map)
          </span>
        </div>

        <div
          className="payment-bar__tally"
          data-testid="payment-bar-tally"
          data-settled={summary.settled}
        >
          <span>
            Paid <strong data-testid="payment-bar-staged">{summary.committed}</strong> / {offer.owed}{" "}
            <ValueUnit kind={kind} />
          </span>
          <span className="text-muted">
            {summary.fromPlanets} from {draft.planetIds.length} planet
            {draft.planetIds.length === 1 ? "" : "s"}
            {offer.hasTradeGoodOption && <>, {draft.tradeGoods} trade good{draft.tradeGoods === 1 ? "" : "s"}</>}
          </span>
          <span
            data-testid="payment-bar-remaining"
            className={summary.settled ? "text-success" : "text-warning"}
          >
            {summary.settled
              ? summary.surplus > 0
                ? `Overpaying by ${summary.surplus}`
                : "Covered"
              : `${summary.shortfall} remaining`}
          </span>
        </div>

        {production.note && (
          <div role="status" className="payment-bar__problem text-warning" data-testid="production-pay-note">
            {production.note}
          </div>
        )}

        {problem && (
          <div role="status" className="payment-bar__problem text-warning" data-testid="payment-bar-problem">
            <ValueText text={problem} />
          </div>
        )}

        <div className="system-activation-bar__actions payment-bar__actions">
          <button
            type="button"
            className="button button--primary"
            data-testid="confirm-payment-btn"
            data-ready={summary.settled && !busy}
            disabled={!summary.settled || busy}
            onClick={() => void confirm()}
          >
            {busy ? "Paying..." : summary.settled ? `Pay (${summary.committed} staged)` : `Stage ${summary.shortfall} more to pay`}
          </button>
          <button
            type="button"
            className="button button--secondary"
            data-testid="auto-pay-btn"
            disabled={busy}
            title="Stage the planets that cover the bill with the least waste; nothing is paid until you confirm"
            onClick={() => {
              const s = suggestAutoPay(offer, tradeGoodsAvailable);
              setDraft({ planetIds: s.planetIds, tradeGoods: s.tradeGoods });
            }}
          >
            Auto-pay
          </button>
          {staged > 0 && (
            <button type="button" className="button button--secondary" disabled={busy} onClick={reset}>
              Reset
            </button>
          )}
          {production.panel && (
            <button
              type="button"
              className="button button--secondary"
              data-testid="ask-each-payment-btn"
              disabled={busy}
              title="Pay this build only; the next builds ask again"
              onClick={() => {
                production.askEach();
                reset();
              }}
            >
              Ask for each build
            </button>
          )}
          <button
            type="button"
            className="button button--secondary"
            data-testid="resume-decision-btn"
            onClick={onOpenList}
          >
            Open decision list
          </button>
          {declineOption && (
            <button
              type="button"
              className="button button--secondary"
              data-testid="decline-payment-btn"
              disabled={busy}
              onClick={() => void onSubmit(declineOption.id)}
            >
              {declineOption.label || "Decline"}
            </button>
          )}
        </div>

        {errorMessage && (
          <div role="alert" className="system-activation-bar__error text-danger" data-testid="payment-bar-error">
            {errorMessage}
          </div>
        )}
      </div>
    </aside>
  );
};
