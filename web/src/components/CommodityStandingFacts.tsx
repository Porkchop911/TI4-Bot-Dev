import React from "react";
import type { PendingChoiceDto } from "../protocol/types.ts";
import {
  describeCommodityStanding,
  type CommodityStanding,
} from "../presentation/commodityStanding.ts";
import "./DecisionContext.css";

/** "Your commodities 1 / 4 · Trade goods 3": what a commodity offer adds to or converts. */
export const CommodityStandingFacts: React.FC<{ standing: CommodityStanding }> = ({ standing }) => {
  const full = standing.have >= standing.max;
  return (
    <dl className="offer-card__facts commodity-standing" data-testid="commodity-standing">
      <div className="offer-card__fact">
        <dt>Your commodities</dt>
        <dd
          data-testid="commodity-standing-have"
          aria-label={`${standing.have} of ${standing.max} commodities${full ? ", full" : ""}`}
        >
          <strong>{standing.have}</strong>
          <span className="text-muted"> / {standing.max}</span>
          {full && <span className="commodity-standing__full"> full</span>}
        </dd>
      </div>
      {standing.tradeGoods !== null && (
        <div className="offer-card__fact">
          <dt>Trade goods</dt>
          <dd data-testid="commodity-standing-goods">{standing.tradeGoods}</dd>
        </div>
      )}
    </dl>
  );
};

/** The standing on its own, above a plain option list (Hacan's agent, Crimson Legionnaire, ...). */
export const CommodityStandingPanel: React.FC<{ choice: Pick<PendingChoiceDto, "details"> }> = ({
  choice,
}) => {
  const standing = describeCommodityStanding(choice);
  if (!standing) return null;
  return (
    <div className="politics-panel" data-testid="commodity-standing-panel">
      <CommodityStandingFacts standing={standing} />
    </div>
  );
};
