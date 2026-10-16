import React, { useState } from "react";
import {
  POOL_LABEL,
  TOKEN_POOLS,
  addToken,
  canAddToken,
  canConfirmTokens,
  confirmBlocker,
  initialStaging,
  maxPurchases,
  purchaseSummary,
  resultingCount,
  stagedPayment,
  suggestedPurchase,
  tokenOutcome,
  tokensRemaining,
  tokensToAssign,
  type CommandTokenView,
  type PurchaseState,
  type TokenOutcome,
} from "../presentation/commandTokens.ts";
import { PlanetValue, ValueUnit } from "./PlanetValueIcons.tsx";
import {
  useSharedTokenDraft,
  useTokenDraftState,
  withPurchase,
} from "../presentation/CommandTokenDraftContext.tsx";
import { planetLabel } from "../presentation/secondaryPlan.ts";

export interface TokenPaymentBarProps {
  view: CommandTokenView;
  onConfirm: (outcome: TokenOutcome) => Promise<void>;
  /** Reopens the token panel. */
  onOpenPanel: () => void;
}

/**
 * The compact bottom bar for Leadership's purchase while the token panel is minimised: the planets
 * that can pay are the controls on the map (the map stays tappable), the bar shows what is
 * selected, buys N tokens and lets the player assign them to the pools and confirm right here, so a
 * confirm control is always reachable. It shares the staged purchase with the panel and the map;
 * nothing is sent before Confirm.
 */
export const TokenPaymentBar: React.FC<TokenPaymentBarProps> = ({ view, onConfirm, onOpenPanel }) => {
  const shared = useSharedTokenDraft();
  const local = useTokenDraftState(null);
  const { draft, update } = shared ?? local;
  const [busy, setBusy] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const [details, setDetails] = useState(false);
  // Collapsed: only the summary and Confirm, so the whole map stays tappable.
  const [collapsed, setCollapsed] = useState(false);
  const purchase = view.purchase;
  if (!purchase) return null;

  const staging = draft.staging ?? initialStaging(view);
  const bought = draft.bought;
  const state: PurchaseState = { bought, override: draft.override };
  const summary = purchaseSummary(view, state);
  const chosen = stagedPayment(view, state);
  const check = shared?.mapPayment?.check ?? null;
  const goods = chosen.tradeGoods;
  const blocker = confirmBlocker(view, staging, bought);
  const ready = canConfirmTokens(view, staging, bought, draft.override) && !busy;
  const remaining = tokensRemaining(view, staging, bought);
  const total = tokensToAssign(view, bought);
  const buyLimit = maxPurchases(view);
  const picked = purchase.planets.filter((planet) => chosen.planetIds.includes(planet.id));
  const selectionProblem =
    (bought > 0 || picked.length > 0 || goods > 0) && check?.problem ? check.problem : null;
  const problem = selectionProblem ?? (remaining === 0 ? blocker : null);

  const confirm = async () => {
    const outcome = tokenOutcome(view, staging, bought, draft.override);
    if (!outcome || busy) return;
    setBusy(true);
    setError(null);
    try {
      await onConfirm(outcome);
    } catch (cause) {
      setError(cause instanceof Error ? cause.message : String(cause));
      // A refusal that names a step to change (the game would not offer the payment) keeps what was staged.
      if (!(cause instanceof Error && cause.name === "PurchaseRejected")) {
        update(() => ({ staging: null, bought: 0, override: null }));
      }
    } finally {
      setBusy(false);
    }
  };

  return (
    <aside
      data-testid="token-payment-bar"
      className="choice-banner panel system-activation-bar payment-bar token-bar"
      aria-label="Payment"
    >
      <div className="system-activation-bar__body token-bar__body">
        <div className="token-bar__summary" data-testid="token-bar-tally" data-settled={bought > 0 && check?.problem === null}>
          {picked.length === 0 && goods === 0 ? (
            <span data-testid="token-bar-hint">
              Tap planets on the map to pay (<ValueUnit kind="influence" /> {purchase.cost} per token)
            </span>
          ) : (
            <span data-testid="token-bar-selected">
              Selected <strong data-testid="token-bar-paid">{summary.influence}</strong> <ValueUnit kind="influence" /> → buys{" "}
              <strong data-testid="token-bar-bought">{bought}</strong> token{bought === 1 ? "" : "s"}
              {summary.wasted > 0 && <span data-testid="token-bar-remaining"> · wasted {summary.wasted}</span>}
              {summary.resourcesLost > 0 && (
                <>
                  {" "}
                  · <PlanetValue kind="resources" value={summary.resourcesLost} label={`${summary.resourcesLost} resources lost this round`} />
                </>
              )}
            </span>
          )}
          <button
            type="button"
            className="button button--secondary button--sm token-bar__toggle"
            data-testid="token-bar-collapse"
            aria-expanded={!collapsed}
            onClick={() => setCollapsed((value) => !value)}
          >
            {collapsed ? "Show ▴" : "Hide ▾"}
          </button>
          <button
            type="button"
            className="button button--secondary button--sm token-bar__toggle"
            data-testid="token-bar-details"
            aria-expanded={details}
            onClick={() => setDetails((open) => !open)}
          >
            {details ? "Planets ▾" : "Planets"}
          </button>
        </div>
        {details && !collapsed && (
          <ul className="token-bar__chips" data-testid="token-bar-chips">
            {picked.length === 0 && <li className="text-muted">None selected</li>}
            {picked.map((planet) => (
              <li key={planet.id} data-testid={`token-bar-chip-${planet.id}`}>
                {planetLabel(planet.id)}{" "}
                <PlanetValue kind="influence" value={planet.worth} label={`Pays ${planet.worth} influence`} />
                {planet.resources !== null && (
                  <PlanetValue
                    kind="resources"
                    value={planet.resources}
                    label={`Exhausting it loses ${planet.resources} resource${planet.resources === 1 ? "" : "s"}`}
                  />
                )}
              </li>
            ))}
          </ul>
        )}
        {!collapsed && (
          // One reserved line, so a message appearing or going never moves the map's planets under the finger.
          <div
            role="status"
            className="payment-bar__problem text-warning token-bar__problem"
            data-testid="token-bar-problem"
            title={problem ?? undefined}
          >
            {problem ?? "\u00a0"}
          </div>
        )}
        {!collapsed && (
          <div className="token-bar__pools" data-testid="token-bar-pools" data-remaining={remaining}>
            <span className="token-bar__pools-label" data-testid="token-bar-assign">
              {total === 0
                ? "Tokens to assign: none yet"
                : remaining > 0
                  ? `Assign ${remaining} of ${total} token${total === 1 ? "" : "s"}:`
                  : "All tokens assigned:"}
            </span>
            {TOKEN_POOLS.map((pool) => (
              <button
                key={pool}
                type="button"
                className="button button--secondary button--sm token-bar__pool"
                data-testid={`token-bar-pool-${pool}`}
                aria-label={`Add a token to ${POOL_LABEL[pool]}`}
                disabled={busy || !canAddToken(view, staging, bought)}
                onClick={() => update((d) => ({ ...d, staging: addToken(view, d.staging ?? staging, pool, d.bought) }))}
              >
                + {POOL_LABEL[pool]} {resultingCount(view, staging, pool)}
              </button>
            ))}
          </div>
        )}
        <div className="system-activation-bar__actions payment-bar__actions token-bar__actions">
          <button
            type="button"
            className="button button--primary"
            data-testid="token-bar-confirm"
            aria-label={bought > 0 ? "Confirm tokens and purchase" : "Confirm tokens"}
            disabled={!ready}
            onClick={() => void confirm()}
          >
            {busy ? "Submitting..." : bought > 0 ? "Confirm purchase" : "Confirm"}
          </button>
          {!collapsed && buyLimit > 0 && (
            <button
              type="button"
              className="button button--secondary"
              data-testid="token-bar-auto"
              disabled={busy || (bought > 0 && draft.override === null)}
              title="Select the suggested planets (planets with no resources first); nothing is paid until you confirm"
              onClick={() => update((d) => withPurchase(view, d, suggestedPurchase(view, { bought: d.bought, override: d.override })))}
            >
              Suggest ({bought > 0 ? bought : buyLimit})
            </button>
          )}
          {!collapsed && purchase.tradeGoods > 0 && (
            <span className="token-panel__stepper" data-testid="token-bar-goods">
              Trade goods {goods}
              <button
                type="button"
                className="button button--secondary button--sm"
                aria-label="Spend one trade good less"
                data-testid="token-bar-goods-minus"
                disabled={busy || goods === 0}
                onClick={() => shared?.stepTradeGoods(-1)}
              >
                −
              </button>
              <button
                type="button"
                className="button button--secondary button--sm"
                aria-label="Spend one trade good more"
                data-testid="token-bar-goods-plus"
                disabled={busy || goods >= purchase.tradeGoods}
                onClick={() => shared?.stepTradeGoods(1)}
              >
                +
              </button>
            </span>
          )}
          {!collapsed && (
          <button
            type="button"
            className="button button--secondary"
            data-testid="resume-choice-button"
            aria-label="Open token panel"
            onClick={onOpenPanel}
          >
            Panel
          </button>
          )}
        </div>
        {error && (
          <div role="alert" className="system-activation-bar__error text-danger" data-testid="token-bar-error">
            {error}
          </div>
        )}
      </div>
    </aside>
  );
};
