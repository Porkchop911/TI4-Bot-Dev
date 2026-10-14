import React, { createContext, useCallback, useContext, useEffect, useMemo, useState } from "react";
import {
  EMPTY_PAYMENT_DRAFT,
  PaymentDraft,
  togglePlanetInDraft,
} from "./paymentDraft.ts";

export interface PaymentDraftApi {
  draft: PaymentDraft;
  togglePlanet: (optionId: string) => void;
  setTradeGoods: (count: number | ((prev: number) => number)) => void;
  setDraft: (draft: PaymentDraft) => void;
  reset: () => void;
}

/** The staged payment, reset whenever the pending decision changes. */
export function usePaymentDraftState(nonce: string | null | undefined): PaymentDraftApi {
  const [draft, setDraft] = useState<PaymentDraft>(EMPTY_PAYMENT_DRAFT);
  useEffect(() => setDraft(EMPTY_PAYMENT_DRAFT), [nonce]);
  const togglePlanet = useCallback(
    (optionId: string) =>
      setDraft((d) => ({ ...d, planetIds: togglePlanetInDraft(d.planetIds, optionId) })),
    [],
  );
  const setTradeGoods = useCallback(
    (count: number | ((prev: number) => number)) =>
      setDraft((d) => ({
        ...d,
        tradeGoods: Math.max(0, typeof count === "function" ? count(d.tradeGoods) : count),
      })),
    [],
  );
  const reset = useCallback(() => setDraft(EMPTY_PAYMENT_DRAFT), []);
  return useMemo(
    () => ({ draft, togglePlanet, setTradeGoods, setDraft, reset }),
    [draft, togglePlanet, setTradeGoods, reset],
  );
}

const PaymentDraftContext = createContext<PaymentDraftApi | null>(null);

export const PaymentDraftProvider: React.FC<{
  value: PaymentDraftApi;
  children: React.ReactNode;
}> = ({ value, children }) => (
  <PaymentDraftContext.Provider value={value}>{children}</PaymentDraftContext.Provider>
);

/** The shared draft (map, bar and drawer), or null outside a provider. */
export const useSharedPaymentDraft = (): PaymentDraftApi | null => useContext(PaymentDraftContext);
