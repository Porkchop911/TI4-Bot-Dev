import React, { useEffect, useMemo, useRef, useState } from "react";
import { PlanetValue, ValueText, ValueUnit } from "./PlanetValueIcons.tsx";
import {
  useSharedTokenDraft,
  useTokenDraftState,
  withPurchase,
} from "../presentation/CommandTokenDraftContext.tsx";
import { planetLabel } from "../presentation/secondaryPlan.ts";
import {
  POOL_LABEL,
  POOL_PURPOSE,
  TOKEN_POOLS,
  addToken,
  canAddToken,
  canConfirmTokens,
  canRemoveToken,
  confirmBlocker,
  chooseTokenCount,
  initialStaging,
  maxPurchases,
  NO_PURCHASE,
  paymentCheck,
  planPayment,
  poolPips,
  purchaseSummary,
  stagedPayment,
  stepPaymentGoods,
  suggestedPurchase,
  togglePaymentPlanet,
  type PurchaseState,
  removeToken,
  resultingCount,
  tokenOutcome,
  tokensRemaining,
  tokensToAssign,
  type CommandTokenView,
  type TokenOutcome,
  type TokenStaging,
  restackMoves,
} from "../presentation/commandTokens.ts";
import "./DecisionContext.css";

export interface CommandTokenPanelProps {
  view: CommandTokenView;
  disabled?: boolean;
  /** Send the staged result: a batch plan for a gain, an option id for a redistribution. */
  onConfirm: (outcome: TokenOutcome) => Promise<void>;
  /** Minimises the panel so planets can be clicked on the map to pay. */
  onPayOnMap?: () => void;
}

const selectedAny = (pay: { planetIds: readonly string[]; tradeGoods: number }): boolean =>
  pay.planetIds.length > 0 || pay.tradeGoods > 0;

const Pips: React.FC<{ kept: number; added: number; removed: number }> = ({
  kept,
  added,
  removed,
}) => (
  <span className="token-panel__pips" aria-hidden="true">
    {Array.from({ length: kept }, (_, i) => (
      <i key={`k${i}`} className="token-panel__pip token-panel__pip--kept" />
    ))}
    {Array.from({ length: added }, (_, i) => (
      <i key={`a${i}`} className="token-panel__pip token-panel__pip--added" />
    ))}
    {Array.from({ length: removed }, (_, i) => (
      <i key={`r${i}`} className="token-panel__pip token-panel__pip--removed" />
    ))}
  </span>
);

/**
 * Command tokens as three pools to fill. Nothing is sent until every token is assigned and the
 * player confirms; a gain goes out as one batch, a redistribution as its single decision.
 */
export const CommandTokenPanel: React.FC<CommandTokenPanelProps> = ({
  view,
  disabled,
  onConfirm,
  onPayOnMap,
}) => {
  const start = useMemo(() => initialStaging(view), [view]);
  // The staged state lives in the shared draft when there is one, so it survives the panel being
  // minimised (to pay on the map) and the map's planet clicks change the same payment.
  const shared = useSharedTokenDraft();
  const local = useTokenDraftState(null);
  const { draft, update } = shared ?? local;
  const staging: TokenStaging = draft.staging ?? start;
  const bought = draft.bought;
  const setStaging = (next: TokenStaging | ((current: TokenStaging) => TokenStaging)) =>
    update((d) => {
      const current = d.staging ?? start;
      return { ...d, staging: typeof next === "function" ? next(current) : next };
    });
  // Planet first: the staged payment decides how many tokens are bought. `override` null is the
  // suggestion for `bought` tokens (nothing when 0).
  const override = draft.override;
  const setPurchase = (next: PurchaseState) => update((d) => withPurchase(view, d, next));
  // "Use suggested planets" fills the payment above; on a phone the assignment is below the fold.
  const assignHeading = useRef<HTMLHeadingElement>(null);
  const scrollToAssign = useRef(false);
  useEffect(() => {
    if (!scrollToAssign.current || !assignHeading.current) return;
    scrollToAssign.current = false;
    assignHeading.current.scrollIntoView?.({ block: "start", behavior: "smooth" });
  });
  const [submitting, setSubmitting] = useState(false);
  const [error, setError] = useState<string | null>(null);
  const locked = disabled || submitting;
  const remaining = tokensRemaining(view, staging, bought);
  const blocker = confirmBlocker(view, staging, bought);
  const changed = bought > 0 || override !== null || TOKEN_POOLS.some((pool) => staging[pool] !== start[pool]);
  const gain = view.mode === "gain";
  const restack = view.mode === "restack";
  const moveCount = restack ? restackMoves(view, staging).length : 0;
  const purchase = view.purchase;
  const buyLimit = maxPurchases(view);
  const check = paymentCheck(view, bought, override);
  const total = tokensToAssign(view, bought);
  const state: PurchaseState = { bought, override };
  const summary = purchaseSummary(view, state);
  const chosen = stagedPayment(view, state);
  const suggestCount = bought > 0 ? bought : buyLimit;
  const suggestion = purchase && suggestCount > 0 ? planPayment(view, suggestCount) : null;
  const suggestedNow = override === null && bought > 0;

  const confirm = async () => {
    const outcome = tokenOutcome(view, staging, bought, override);
    if (!outcome) return;
    setSubmitting(true);
    setError(null);
    try {
      await onConfirm(outcome);
    } catch (cause) {
      // The engine moved on or rejected the plan: start again from what it offers now.
      setError(cause instanceof Error ? cause.message : String(cause));
      // A refusal that names a step to change (the game would not offer the payment) keeps what was staged.
      if (!(cause instanceof Error && cause.name === "PurchaseRejected")) {
        update((d) => ({ ...d, staging: start, bought: 0, override: null }));
      }
    } finally {
      setSubmitting(false);
    }
  };

  return (
    <div className="token-panel" data-testid="command-token-panel" data-mode={view.mode}>
      <div className="token-panel__summary">
        <span data-testid="token-total">
          {gain ? "Tokens to assign" : "Tokens to arrange"} <strong>{total}</strong>
          {purchase && (
            <span className="token-panel__split" data-testid="token-split">
              {" "}
              ({view.total} free + {bought} bought)
            </span>
          )}
        </span>
        <span
          className="token-panel__remaining"
          data-testid="token-remaining"
          data-done={remaining === 0}
        >
          Remaining <strong>{remaining}</strong>
        </span>
        {view.reinforcements !== null && (
          <span data-testid="token-reinforcements">
            In reinforcements <strong>{view.reinforcements}</strong>
          </span>
        )}
      </div>
      {restack && (
        <p className="token-panel__note" data-testid="token-restack-note">
          Predictive Intelligence: arrange the pools you want at the end of your turn. The{" "}
          {moveCount === 0 ? "moves" : `${moveCount} move${moveCount === 1 ? "" : "s"}`} needed are
          sent one after another, then the redistribution ends.
        </p>
      )}
      {purchase && (
        <div className="token-panel__buy" data-testid="token-buy">
          <h3 className="token-panel__step">
            1 · Pay with planets
            <span className="token-panel__purpose">
              <PlanetValue kind="influence" value={purchase.cost} /> per command token, up to {buyLimit}
            </span>
          </h3>
          <ul className="token-panel__planets" data-testid="token-payment-planets">
            {purchase.planets.map((planet) => {
              const picked = chosen.planetIds.includes(planet.id);
              return (
                <li key={planet.id}>
                  <button
                    type="button"
                    className="button button--secondary token-panel__planet"
                    data-testid={`token-payment-planet-${planet.id}`}
                    aria-pressed={picked}
                    disabled={locked}
                    onClick={() => setPurchase(togglePaymentPlanet(view, state, planet.id))}
                  >
                    <span className="token-panel__planet-name">{planetLabel(planet.id)}</span>
                    <PlanetValue
                      kind="influence"
                      value={planet.worth}
                      label={`Pays ${planet.worth} influence`}
                    />
                    {planet.resources !== null && (
                      <PlanetValue
                        kind="resources"
                        value={planet.resources}
                        label={`Exhausting it loses ${planet.resources} resource${planet.resources === 1 ? "" : "s"}`}
                        testId={`token-payment-planet-${planet.id}-resources`}
                      />
                    )}
                    <span className="token-panel__planet-state">{picked ? "✓ exhaust" : "ready"}</span>
                  </button>
                </li>
              );
            })}
          </ul>
          {purchase.tradeGoods > 0 && (
            <span className="token-panel__stepper" data-testid="token-payment-goods">
              Trade goods (<PlanetValue kind="influence" value={purchase.tradeGoodWorth} /> each, {purchase.tradeGoods} held)
              <button
                type="button"
                className="button button--secondary button--sm"
                data-testid="token-payment-goods-minus"
                aria-label="Spend one trade good less"
                disabled={locked || chosen.tradeGoods === 0}
                onClick={() => setPurchase(stepPaymentGoods(view, state, -1))}
              >
                −
              </button>
              <span className="token-panel__count" data-testid="token-payment-goods-count">
                {chosen.tradeGoods}
              </span>
              <button
                type="button"
                className="button button--secondary button--sm"
                data-testid="token-payment-goods-plus"
                aria-label="Spend one trade good more"
                disabled={locked || chosen.tradeGoods >= purchase.tradeGoods}
                onClick={() => setPurchase(stepPaymentGoods(view, state, 1))}
              >
                +
              </button>
            </span>
          )}
          <p className="token-panel__tally" data-testid="token-purchase-summary" data-bought={bought} data-wasted={summary.wasted}>
            Influence selected <strong data-testid="token-influence-spent">{summary.influence}</strong>
            {" "}<ValueUnit kind="influence" /> → buys <strong data-testid="token-buy-count-summary">{bought}</strong>{" "}
            token{bought === 1 ? "" : "s"} ({purchase.cost} each), wasted <strong data-testid="token-wasted">{summary.wasted}</strong>
            {summary.resourcesLost > 0 && (
              <span data-testid="token-resources-lost">
                {" "}
                · exhausts <PlanetValue kind="resources" value={summary.resourcesLost} label={`${summary.resourcesLost} resources lost this round`} />
              </span>
            )}
          </p>
          {summary.atLimit && (
            <p className="token-panel__note" data-testid="token-limit-note">
              The most you can buy is {buyLimit} (influence or reinforcements); take a planet out.
            </p>
          )}
          {check.problem && (selectedAny(chosen) || bought > 0) && (
            <p className="token-panel__note" role="alert" data-testid="token-payment-problem">
              <ValueText text={check.problem} />
            </p>
          )}
          <p className="token-panel__note token-panel__note--quiet" data-testid="token-payment-account" data-ok={check.problem === null}>
            Paid <strong>{check.paid}</strong> · owed <strong>{check.bill}</strong> · remainder{" "}
            <strong>{check.remainder}</strong> · waste <strong>{check.waste}</strong> <ValueUnit kind="influence" />
            {" "}· <ValueUnit kind="influence" /> available <strong>{purchase.influence}</strong>
          </p>
          <div className="token-panel__buy-actions">
            {buyLimit > 0 && (
              <button
                type="button"
                className="button button--primary button--sm token-panel__action"
                data-testid="token-payment-auto"
                disabled={locked || suggestedNow}
                onClick={() => {
                  scrollToAssign.current = true;
                  setPurchase(suggestedPurchase(view, state));
                }}
              >
                Use suggested planets (buys {suggestCount})
              </button>
            )}
            <button
              type="button"
              className="button button--secondary button--sm token-panel__action"
              data-testid="token-payment-clear"
              disabled={locked || (!selectedAny(chosen) && bought === 0)}
              onClick={() => setPurchase(NO_PURCHASE)}
            >
              Clear selection
            </button>
            {onPayOnMap && (
              <button
                type="button"
                className="button button--secondary button--sm token-panel__action"
                data-testid="token-pay-on-map"
                disabled={locked}
                onClick={onPayOnMap}
              >
                Select on the map
              </button>
            )}
          </div>
          {suggestion && (
            <p className="token-panel__note token-panel__note--quiet" data-testid="token-suggestion">
              Suggested for {suggestCount} token{suggestCount === 1 ? "" : "s"}:{" "}
              {suggestion.planets.map((planet, i) => (
                <span key={planet.id} data-testid={`token-suggestion-${planet.id}`}>
                  {i > 0 ? ", " : ""}
                  {planetLabel(planet.id)}{" "}
                  <PlanetValue kind="influence" value={planet.worth} label={`Pays ${planet.worth} influence`} />
                  {planet.resources !== null && (
                    <PlanetValue
                      kind="resources"
                      value={planet.resources}
                      label={`Exhausting it loses ${planet.resources} resource${planet.resources === 1 ? "" : "s"}`}
                    />
                  )}
                </span>
              ))}
              {suggestion.tradeGoods > 0 &&
                `${suggestion.planets.length ? " + " : ""}${suggestion.tradeGoods} trade good${suggestion.tradeGoods === 1 ? "" : "s"}`}
              . Planets with no resources go first.
            </p>
          )}
          <div className="token-panel__quick" data-testid="token-quick">
            <span>Or choose how many to buy first</span>
            <span className="token-panel__stepper">
              <button
                type="button"
                className="button button--secondary button--sm"
                data-testid="token-buy-minus"
                aria-label="Buy one token less"
                disabled={locked || bought === 0}
                onClick={() => setPurchase(chooseTokenCount(view, bought - 1))}
              >
                −
              </button>
              <span className="token-panel__count" data-testid="token-buy-count" aria-label="Tokens bought">
                {bought}
              </span>
              <button
                type="button"
                className="button button--secondary button--sm"
                data-testid="token-buy-plus"
                aria-label="Buy one token more"
                disabled={locked || bought >= buyLimit}
                onClick={() => setPurchase(chooseTokenCount(view, bought + 1))}
              >
                +
              </button>
            </span>
          </div>
        </div>
      )}
      {purchase && (
        <h3 className="token-panel__step" data-testid="token-assign-heading" ref={assignHeading}>
          2 · Assign the {total} token{total === 1 ? "" : "s"}
          <span className="token-panel__purpose">
            {view.total > 0 ? `${view.total} free + ${bought} bought` : `${bought} bought`}
          </span>
        </h3>
      )}
      <div className="token-panel__pools">
        {TOKEN_POOLS.map((pool) => {
          const pips = poolPips(view, staging, pool);
          const label = POOL_LABEL[pool];
          return (
            <div
              key={pool}
              className="token-panel__pool"
              data-testid={`token-pool-${pool}`}
              data-staged={staging[pool]}
            >
              <span className="token-panel__name">
                {label}
                <span className="token-panel__purpose">{POOL_PURPOSE[pool]}</span>
              </span>
              <span className="token-panel__stepper">
                <button
                  type="button"
                  className="button button--secondary button--sm"
                  data-testid={`token-minus-${pool}`}
                  aria-label={`Remove a token from ${label}`}
                  disabled={locked || !canRemoveToken(staging, pool)}
                  onClick={() => setStaging((s) => removeToken(s, pool))}
                >
                  −
                </button>
                <span
                  className="token-panel__count"
                  data-testid={`token-count-${pool}`}
                  aria-label={`${label} tokens`}
                >
                  {resultingCount(view, staging, pool)}
                </span>
                <button
                  type="button"
                  className="button button--secondary button--sm"
                  data-testid={`token-plus-${pool}`}
                  aria-label={`Add a token to ${label}`}
                  disabled={locked || !canAddToken(view, staging, bought)}
                  onClick={() => setStaging((s) => addToken(view, s, pool, bought))}
                >
                  +
                </button>
              </span>
              <span data-testid={`token-pips-${pool}`} data-kept={pips.kept} data-added={pips.added} data-removed={pips.removed}>
                <Pips {...pips} />
              </span>
            </div>
          );
        })}
      </div>
      <div className="token-panel__legend" aria-hidden="true">
        <span>
          <i className="token-panel__pip token-panel__pip--kept" /> already there
        </span>
        <span>
          <i className="token-panel__pip token-panel__pip--added" />{" "}
          {gain ? "newly assigned" : "moved in"}
        </span>
        {!gain && (
          <span>
            <i className="token-panel__pip token-panel__pip--removed" /> moved out
          </span>
        )}
      </div>
      {blocker && (
        <p className="token-panel__note" data-testid="token-blocker">
          {blocker}
        </p>
      )}
      <div className="token-panel__actions">
        <button
          type="button"
          className="button button--secondary"
          data-testid="token-reset"
          disabled={locked || !changed}
          onClick={() => {
            update((d) => ({ ...d, staging: start, bought: 0, override: null }));
          }}
        >
          Reset
        </button>
        <button
          type="button"
          className="button button--primary"
          data-testid="token-confirm"
          disabled={locked || !canConfirmTokens(view, staging, bought, override)}
          onClick={() => void confirm()}
        >
          {submitting
            ? "Submitting..."
            : restack
              ? moveCount === 0
                ? "Keep as is"
                : `Confirm ${moveCount} move${moveCount === 1 ? "" : "s"}`
              : !gain
              ? "Confirm arrangement"
              : total === 0
                ? "No purchase"
                : bought > 0
                  ? "Confirm tokens and purchase"
                  : "Confirm tokens"}
        </button>
      </div>
      {error && (
        <div className="hit-assignment__error" role="alert" data-testid="token-error">
          {error}
        </div>
      )}
    </div>
  );
};
