import React from "react";
import type { ChoiceOptionDto, PendingChoiceDto } from "../protocol/types.ts";
import { findPlanetMeta, humanizeId } from "../protocol/contentCatalog.ts";
import { describeLawOption, isLawPick } from "../presentation/lawPick.ts";
import { SeatTag } from "./SeatTag.tsx";
import "./ActionCardPickers.css";

/** Above the laws of Repeal Law: the card, and how many laws are in play. */
export const LawPickPanel: React.FC<{ choice: PendingChoiceDto }> = ({ choice }) => {
  if (!isLawPick(choice)) return null;
  const count = choice.options.length;
  return (
    <div className="politics-panel" data-testid="law-pick-panel">
      <div className="politics-panel__title">
        Repeal Law
        <span className="politics-panel__tag">discard a law</span>
      </div>
      <p className="text-muted">
        Choose 1 of the {count} laws in play. It is discarded and stops applying; the agenda goes to the discard pile.
      </p>
    </div>
  );
};

const looksLikeSeat = (id: string) => /^player_/.test(id);

/** Inside a law's row: its kind, what it was elected onto and the printed text that is in force. */
export const LawPickOptionNote: React.FC<{ choice: PendingChoiceDto; option: ChoiceOptionDto }> = ({
  choice,
  option,
}) => {
  const law = describeLawOption(choice, option);
  if (!law) return null;
  const electedName = law.elected
    ? (findPlanetMeta(law.elected)?.name ?? (looksLikeSeat(law.elected) ? null : humanizeId(law.elected)))
    : null;
  return (
    <div className="law-note" data-testid="law-pick-note">
      <div className="law-note__tags">
        {law.kind && <span className="politics-panel__tag law-note__tag">{law.kind}</span>}
        {law.passed && (
          <span className="politics-panel__tag law-note__tag" data-testid="law-passed">
            Passed: {law.passed}
          </span>
        )}
        {law.elected && (
          <span className="law-note__elected" data-testid="law-elected">
            Elected: {looksLikeSeat(law.elected) ? <SeatTag seat={law.elected} /> : (electedName ?? law.elected)}
          </span>
        )}
      </div>
      {law.text.map((line) => (
        <p key={line} className="politics-panel__text" data-testid="law-pick-text">
          {line}
        </p>
      ))}
    </div>
  );
};
