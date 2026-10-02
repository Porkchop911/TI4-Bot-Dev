import React, { useState, useEffect, useMemo } from "react";
import { PendingChoiceDto } from "../protocol/types.ts";
import {
  decodeTradeOption,
  DecodedTradeOffer,
  TradeCategory,
} from "../presentation/tradeDecoder.ts";
import { Dialog } from "../primitives/index.ts";
import { ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { WorkflowShell } from "./WorkflowShell.tsx";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { DecisionHeader } from "./DecisionHeader.tsx";

export interface TradeDeskModalProps {
  choice: PendingChoiceDto | null;
  model?: ChoiceRendererModel | null;
  viewerSeat?: string | null;
  onSubmit: (optionId: string) => Promise<void>;
  isOpen: boolean;
  onClose: () => void;
  lastError?: string | null;
}

const CATEGORY_NAMES: Record<TradeCategory, string> = {
  commodity_swap: "Commodities",
  goods_exchange: "Goods Exchange",
  promissory: "Promissory Notes",
  mutual_support: "Mutual Support",
  other: "Special Offers",
};

export const TradeDeskModal: React.FC<TradeDeskModalProps> = ({
  choice,
  model,
  viewerSeat,
  onSubmit,
  isOpen,
  onClose,
  lastError,
}) => {
  const display = usePlayerIdentity();
  const subtype = choice?.context?.subtype ?? "";
  const isAnswering = subtype === "answer_transaction" || model?.workflow === "transaction_answer";
  // Use centralized model for partner seat when available
  const partnerSeat =
    (model?.selectionMode.mode === "transaction" ? model.selectionMode.partnerSeat : null) ??
    (choice?.context?.target && "Player" in choice.context.target
      ? choice.context.target.Player
      : null);

  const [activeTab, setActiveTab] = useState<TradeCategory>("commodity_swap");
  const [selectedOfferId, setSelectedOfferId] = useState<string | null>(null);

  // Parse all propose offers
  const decodedOffers = useMemo<DecodedTradeOffer[]>(() => {
    if (!choice || isAnswering) return [];
    return choice.options
      .filter((o) => o.id !== "decline" && o.kind !== "decline")
      .map(decodeTradeOption);
  }, [choice, isAnswering]);

  // Available categories
  const availableCategories = useMemo<TradeCategory[]>(() => {
    const categories = new Set<TradeCategory>();
    for (const off of decodedOffers) {
      categories.add(off.category);
    }
    const order: TradeCategory[] = [
      "commodity_swap",
      "goods_exchange",
      "promissory",
      "mutual_support",
      "other",
    ];
    return order.filter((c) => categories.has(c));
  }, [decodedOffers]);

  // Ensure activeTab is valid when options change
  useEffect(() => {
    if (availableCategories.length > 0 && !availableCategories.includes(activeTab)) {
      setActiveTab(availableCategories[0]);
    }
  }, [availableCategories, activeTab]);

  // Reset selection on nonce change
  useEffect(() => {
    setSelectedOfferId(null);
  }, [choice?.nonce]);

  const currentTabOffers = decodedOffers.filter((o) => o.category === activeTab);
  const selectedOffer = decodedOffers.find((o) => o.id === selectedOfferId) ?? null;

  if (!isOpen || !choice) return null;

  return (
    <Dialog.Root
      open={isOpen}
      onOpenChange={(open) => {
        if (!open) onClose();
      }}
    >
      <Dialog.Content
        data-testid="trade-desk-modal"
        className="trade-dialog choice-workflow-dialog"
      >
        <div className="panel choice-workflow-modal">
          <Dialog.Title as="h2" className="visually-hidden">
            {choice.prompt}
          </Dialog.Title>
          <DecisionHeader
            actor={choice.actor}
            title={isAnswering ? "Answer the trade offer" : "Propose a trade"}
            instruction={choice.prompt}
            progress={
              partnerSeat && display(partnerSeat).position
                ? `With ${display(partnerSeat).label}`
                : undefined
            }
            onMinimize={onClose}
            titleTestId="trade-desk-title"
            minimizeTestId="close-trade-modal"
          />

          <WorkflowShell
            choice={choice}
            model={model}
            viewerSeat={viewerSeat}
            onSubmit={onSubmit}
            lastError={lastError}
            spectatorNotice={`Observing bilateral trade negotiations between ${display(choice.actor).label} and ${display(partnerSeat).label}...`}
            spectatorNoticeTestId="spectator-trade-notice"
            errorTestId="trade-error-banner"
          >
            {({ isActor, isDirectSubmitting: isSubmitting, declineOption, submitDirect }) => (
              <>
                {/* Answer Mode */}
                {isActor && isAnswering && (
                  <div className="workflow-inline">
                    <div className="workflow-inline">
                      {choice.options.map((opt) => {
                        const isAccept = opt.id === "accept";
                        const isRefuse = opt.id === "refuse" || opt.kind === "decline";
                        const isCounter = opt.id === "counter";

                        const answerClass = isAccept
                          ? "trade-dialog__answer--accept"
                          : isRefuse
                            ? "trade-dialog__answer--refuse"
                            : isCounter
                              ? "trade-dialog__answer--counter"
                              : "";

                        return (
                          <button
                            key={opt.id}
                            type="button"
                            data-testid={`answer-opt-${opt.id}`}
                            onClick={() => submitDirect(opt.id)}
                            disabled={isSubmitting}
                            className={`button ${isAccept ? "button--primary" : "button--secondary"} ${answerClass}`}
                          >
                            {opt.label}
                          </button>
                        );
                      })}
                    </div>
                  </div>
                )}

                {/* Propose Mode */}
                {isActor && !isAnswering && (
                  <div className="workflow-inline">
                    {/* Category Tabs */}
                    {availableCategories.length > 0 && (
                      <div role="tablist" className="workflow-inline">
                        {availableCategories.map((cat) => (
                          <button
                            key={cat}
                            role="tab"
                            aria-selected={activeTab === cat}
                            data-testid={`trade-tab-${cat}`}
                            type="button"
                            onClick={() => setActiveTab(cat)}
                            className={`button ${activeTab === cat ? "button--primary" : "button--secondary"}`}
                          >
                            {CATEGORY_NAMES[cat]}
                          </button>
                        ))}
                      </div>
                    )}

                    {/* Offer Options List */}
                    <div className="workflow-inline">
                      {currentTabOffers.length === 0 ? (
                        <div className="workflow-inline">No available offers in this category.</div>
                      ) : (
                        currentTabOffers.map((offer) => {
                          const isSelected = selectedOfferId === offer.id;
                          return (
                            <button
                              key={offer.id}
                              type="button"
                              data-testid={`trade-opt-${offer.id}`}
                              onClick={() => setSelectedOfferId(offer.id)}
                              className="button button--secondary trade-dialog__offer"
                              data-selected={isSelected}
                            >
                              <span className="workflow-inline">{offer.label}</span>
                              {offer.net !== undefined && (
                                <span className="workflow-inline">
                                  {offer.net >= 0 ? `+${offer.net}` : offer.net} Value
                                </span>
                              )}
                            </button>
                          );
                        })
                      )}
                    </div>

                    {/* Selected Offer Summary */}
                    {selectedOffer && (
                      <div data-testid="selected-trade-summary" className="workflow-inline">
                        <span>
                          Selected Deal: <strong>{selectedOffer.label}</strong>
                        </span>
                        {selectedOffer.net !== undefined && (
                          <span>
                            Give / receive net:{" "}
                            <strong>
                              {selectedOffer.net >= 0 ? `+${selectedOffer.net}` : selectedOffer.net}
                            </strong>
                          </span>
                        )}
                      </div>
                    )}

                    {/* Actions */}
                    <div className="workflow-inline">
                      {selectedOffer && (
                        <button
                          type="button"
                          className="button button--secondary"
                          onClick={() => setSelectedOfferId(null)}
                        >
                          Reset selection
                        </button>
                      )}
                      {declineOption ? (
                        <button
                          type="button"
                          data-testid="decline-trade-btn"
                          onClick={() => submitDirect(declineOption.id)}
                          disabled={isSubmitting}
                          className="button button--secondary"
                        >
                          {declineOption.label || "Offer Nothing"}
                        </button>
                      ) : (
                        <div />
                      )}

                      <button
                        type="button"
                        data-testid="propose-trade-btn"
                        onClick={() => selectedOfferId && submitDirect(selectedOfferId)}
                        disabled={!selectedOfferId || isSubmitting}
                        className="button button--primary"
                      >
                        {isSubmitting ? "Proposing..." : "Propose Deal"}
                      </button>
                    </div>
                  </div>
                )}
              </>
            )}
          </WorkflowShell>
        </div>
      </Dialog.Content>
    </Dialog.Root>
  );
};
