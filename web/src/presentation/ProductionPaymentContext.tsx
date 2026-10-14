import React, { createContext, useContext, useEffect, useMemo, useRef } from "react";
import type { PendingChoiceDto } from "../protocol/types.ts";
import {
  suggestAutoPay,
  type PaymentDraft,
  type PaymentOffer,
} from "./paymentDraft.ts";
import {
  allocateQuestion,
  isProductionPayQuestion,
  planPanel,
  type PlanPanel,
  type ProductionPay,
} from "./productionPayment.ts";

const EMPTY_OFFER: PaymentOffer = {
  planets: [],
  hasTradeGoodOption: false,
  tradeGoodWorth: 1,
  owed: 0,
  totalAmount: 0,
  alreadyPaid: 0,
  currency: "Resources",
};

export interface ProductionPaymentApi {
  plan: ProductionPay | null;
  /** The player confirmed the one payment: `rest` is what the later builds are paid with. */
  confirm: (rest: PaymentDraft, nonce: string) => void;
  /** The confirmed first payment was refused: ask for the whole production again. */
  revert: () => void;
  /** Stop paying from a plan: ask build by build. */
  askEach: (note?: string) => void;
}

const ProductionPaymentContext = createContext<ProductionPaymentApi | null>(null);

export const ProductionPaymentProvider: React.FC<{
  value: ProductionPaymentApi;
  children: React.ReactNode;
}> = ({ value, children }) => (
  <ProductionPaymentContext.Provider value={value}>{children}</ProductionPaymentContext.Provider>
);

export const useProductionPayment = (): ProductionPaymentApi | null =>
  useContext(ProductionPaymentContext);

export interface ProductionPaymentPanel {
  /** The one panel for the whole production is open for this question. */
  panel: PlanPanel | null;
  /** The offer to show and stage against: the whole production's bill in the panel. */
  offer: PaymentOffer;
  /** Why an earlier plan stopped (shown above the payment that is asked by hand). */
  note: string | null;
  /**
   * Splits the staged payment for the open question. Without a panel the staged payment is
   * returned as it is. Null when the staged payment cannot pay the open question.
   */
  split: (draft: PaymentDraft) => PaymentDraft | null;
  /** Ends the panel (confirm went through or was refused). */
  revert: () => void;
  askEach: () => void;
}

/**
 * What the payment drawer and bar need for the production's single payment: the whole bill as the
 * offer, the suggestion prefilled once per question, and the split of the staged payment into
 * this question's part and the plan for the later ones.
 */
export function useProductionPaymentPanel(
  choice: PendingChoiceDto | null,
  base: PaymentOffer | null,
  tradeGoodsAvailable: number,
  draft: PaymentDraft,
  setDraft: (draft: PaymentDraft) => void,
): ProductionPaymentPanel {
  const ctx = useProductionPayment();
  const plan = ctx?.plan ?? null;
  const panel = useMemo(
    () => (choice && base ? planPanel(choice, base, plan) : null),
    [choice, base, plan],
  );
  const effective = panel?.offer ?? base ?? EMPTY_OFFER;

  // The suggestion is staged once per question: the whole bill in the panel, or the open
  // question's bill when the plan stopped and the player is asked build by build.
  const prefillable = Boolean(
    choice && (panel || (plan?.mode === "manual" && isProductionPayQuestion(choice, plan))),
  );
  const latest = useRef({ draft, effective, tradeGoodsAvailable, setDraft });
  latest.current = { draft, effective, tradeGoodsAvailable, setDraft };
  const nonce = choice?.nonce;
  useEffect(() => {
    if (!prefillable) return;
    // After the shared draft's own reset for a new question.
    const timer = setTimeout(() => {
      const now = latest.current;
      if (now.draft.planetIds.length || now.draft.tradeGoods) return;
      const suggestion = suggestAutoPay(now.effective, now.tradeGoodsAvailable);
      if (suggestion.planetIds.length || suggestion.tradeGoods)
        now.setDraft({ planetIds: suggestion.planetIds, tradeGoods: suggestion.tradeGoods });
    }, 0);
    return () => clearTimeout(timer);
  }, [nonce, prefillable]);

  return {
    panel,
    offer: effective,
    note: plan?.note && choice && isProductionPayQuestion(choice, plan) ? plan.note : null,
    split: (staged) => {
      if (!panel || !base || !choice) return staged;
      const allocation = allocateQuestion(base, staged, tradeGoodsAvailable);
      if (!allocation) return null;
      ctx?.confirm(allocation.rest, choice.nonce);
      return allocation.draft;
    },
    revert: () => ctx?.revert(),
    askEach: () => ctx?.askEach(),
  };
}
