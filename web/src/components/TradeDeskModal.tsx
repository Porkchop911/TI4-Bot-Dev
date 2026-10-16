import React, { useState, useEffect, useMemo, useRef } from "react";
import { PendingChoiceDto, PlayerView } from "../protocol/types.ts";
import {
  DecodedTradeOffer,
  TradeCategory,
} from "../presentation/tradeDecoder.ts";
import { Dialog } from "../primitives/index.ts";
import { ChoiceRendererModel } from "../presentation/choiceModel.ts";
import { humanizeOfferPrompt } from "../presentation/tradeNames.ts";
import { WorkflowShell } from "./WorkflowShell.tsx";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { DecisionHeader } from "./DecisionHeader.tsx";
import {
  Staged,
  emptyStaged,
  parseOfferPrompt,
  proposalId,
  stageableDeals,
  stagedToDealId,
} from "../presentation/tradeStaging.ts";
import {
  answeringACounter,
  forgetNegotiation,
  rememberCounter,
  rememberProposal,
  takeCounterPrefill,
} from "../presentation/tradeNegotiation.ts";
import { TradeStagingDesk, TradeOfferColumns } from "./TradeStagingDesk.tsx";

export interface TradeDeskModalProps {
  choice: PendingChoiceDto | null;
  model?: ChoiceRendererModel | null;
  viewerSeat?: string | null;
  onSubmit: (optionId: string) => Promise<void>;
  isOpen: boolean;
  onClose: () => void;
  lastError?: string | null;
  /** Public table state: the holdings shown on each side of the desk. */
  players?: Record<string, PlayerView>;
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
  players,
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
  const [staged, setStaged] = useState<Staged>(emptyStaged);
  const [prefilled, setPrefilled] = useState(false);
  const stagedNonce = useRef<string | null>(null);

  // The server's deal list: the only things that can be proposed.
  const deals = useMemo(
    () => (choice && !isAnswering ? stageableDeals(choice.options) : []),
    [choice, isAnswering],
  );
  const decodedOffers = useMemo<DecodedTradeOffer[]>(() => deals.map((d) => d.deal), [deals]);

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

  // A new decision starts a clean desk, unless a counter-offer is waiting to be pre-filled. The ref
  // keeps a re-run of this effect (strict mode) from wiping what the first run staged.
  useEffect(() => {
    const nonce = choice?.nonce ?? null;
    if (stagedNonce.current === nonce) return;
    stagedNonce.current = nonce;
    const carried = !isAnswering ? takeCounterPrefill(partnerSeat) : null;
    setStaged(carried ?? emptyStaged());
    setPrefilled(carried !== null);
  }, [choice?.nonce, isAnswering, partnerSeat]);

  const currentTabOffers = decodedOffers.filter((o) => o.category === activeTab);
  const freeForm = typeof choice?.details?.free_form === "string";
  const selectedId = stagedToDealId(staged, deals);
  const offered = useMemo(
    () => (choice && isAnswering ? parseOfferPrompt(choice.prompt) : null),
    [choice, isAnswering],
  );
  const shownPrompt = choice ? humanizeOfferPrompt(choice.prompt) : "";
  const viewer = players?.[viewerSeat ?? choice?.actor ?? ""];
  const partner = partnerSeat ? players?.[partnerSeat] : undefined;
  const holdings = (p?: PlayerView) =>
    p ? { tradeGoods: p.trade_goods, commodities: p.commodities } : null;
  const partnerLabel = partnerSeat ? display(partnerSeat).label : "your partner";
  const submitId = proposalId(staged, deals, freeForm, holdings(viewer), holdings(partner));
  const acceptNet = choice?.options.find((o) => o.id === "accept")?.payload?.net;

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
            {shownPrompt}
          </Dialog.Title>
          <DecisionHeader
            actor={choice.actor}
            choice={choice}
            title={isAnswering ? "Answer the trade offer" : "Propose a trade"}
            instruction={shownPrompt}
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
                {/* Answer Mode: the partner's offer in two columns, then the three answers */}
                {isActor && isAnswering && (
                  <div className="trade-desk-flow" data-testid="trade-answer">
                    {offered ? (
                      <TradeOfferColumns
                        staged={offered.staged}
                        proposerLabel={partnerLabel}
                        net={typeof acceptNet === "number" ? acceptNet : undefined}
                      />
                    ) : (
                      <p className="trade-column__none" data-testid="trade-offer-unparsed">
                        The offer is described in the sentence above.
                      </p>
                    )}
                    {choice.options.some((o) => o.id === "counter") && (
                      <p className="trade-desk__note" data-testid="counter-note">
                        {answeringACounter(partnerSeat)
                          ? "This is the answer to your counter-offer: choosing Counter-offer again ends the negotiation."
                          : "The engine allows exactly one counter-offer. Counter-offer reopens the desk with this deal pre-filled so you can edit it; the other player then answers it."}
                      </p>
                    )}
                    <div className="trade-desk__actions" data-testid="trade-answer-actions">
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
                            onClick={() => {
                              if (isCounter && offered && partnerSeat) {
                                rememberCounter(partnerSeat, offered.staged);
                              } else if (isAccept || isRefuse) {
                                forgetNegotiation();
                              }
                              void submitDirect(opt.id);
                            }}
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

                {/* Propose Mode: the staging desk, with the listed deals as a fallback */}
                {isActor && !isAnswering && (
                  <div className="trade-desk-flow">
                    <TradeStagingDesk
                      deals={deals}
                      staged={staged}
                      onChange={(next) => {
                        setStaged(next);
                        setPrefilled(false);
                      }}
                      partnerLabel={partnerLabel}
                      partnerFaction={partner?.faction ?? null}
                      myHoldings={holdings(viewer)}
                      partnerHoldings={holdings(partner)}
                      freeForm={freeForm}
                      banner={
                        prefilled ? (
                          <p className="trade-desk__note" data-testid="counter-prefill-note">
                            Counter-offer: {partnerLabel}&apos;s deal is staged below as you would
                            propose it. Edit it, then propose; this is your one counter.
                          </p>
                        ) : null
                      }
                    />

                    {/* Quick deals: the full listed catalogue, one click each */}
                    <details className="trade-quick" data-testid="quick-deals">
                      <summary>Quick deals ({decodedOffers.length} listed)</summary>
                      {availableCategories.length > 0 && (
                        <div role="tablist" className="trade-dialog__tabs">
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
                      <div className="trade-dialog__offers">
                        {currentTabOffers.length === 0 ? (
                          <div>No available offers in this category.</div>
                        ) : (
                          currentTabOffers.map((offer) => (
                            <button
                              key={offer.id}
                              type="button"
                              data-testid={`trade-opt-${offer.id}`}
                              onClick={() => {
                                const d = deals.find((x) => x.id === offer.id);
                                if (d) setStaged(d.staged);
                                setPrefilled(false);
                              }}
                              className="button button--secondary trade-dialog__offer"
                              data-selected={selectedId === offer.id}
                            >
                              <span title={offer.engineLabel !== offer.label ? offer.engineLabel : undefined}>{offer.label}</span>
                              {offer.net !== undefined && (
                                <span>{offer.net >= 0 ? `+${offer.net}` : offer.net} Value</span>
                              )}
                            </button>
                          ))
                        )}
                      </div>
                    </details>

                    {/* Actions: sticky on phones */}
                    <div className="trade-desk__actions" data-testid="trade-propose-actions">
                      {declineOption ? (
                        <button
                          type="button"
                          data-testid="decline-trade-btn"
                          onClick={() => {
                            forgetNegotiation();
                            void submitDirect(declineOption.id);
                          }}
                          disabled={isSubmitting}
                          className="button button--secondary"
                        >
                          {declineOption.label || "Offer Nothing"}
                        </button>
                      ) : null}
                      <button
                        type="button"
                        data-testid="reset-staging-btn"
                        className="button button--secondary"
                        onClick={() => {
                          setStaged(emptyStaged());
                          setPrefilled(false);
                        }}
                      >
                        Clear desk
                      </button>
                      <button
                        type="button"
                        data-testid="propose-trade-btn"
                        onClick={() => {
                          if (!submitId) return;
                          rememberProposal(partnerSeat);
                          void submitDirect(submitId);
                        }}
                        disabled={!submitId || isSubmitting}
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
