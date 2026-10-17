import React from "react";
import type { ChoiceOption, PendingChoiceDto } from "../protocol/types.ts";
import { describeTechnologyPickOption, technologyPickStage } from "../presentation/technologyPick.ts";
import { SeatTag } from "./SeatTag.tsx";

/** Above the options of Divert Funding: which half of the card this is and what it does. */
export const TechnologyPickPanel: React.FC<{ choice: PendingChoiceDto }> = ({ choice }) => {
  const stage = technologyPickStage(choice);
  if (!stage) return null;
  return (
    <div className="politics-panel" data-testid="technology-pick-panel">
      <div className="politics-panel__title">
        {choice.context?.subtype === "starting_technology" ? (
          <span className="politics-panel__tag" data-testid="starting-technology-title">
            {stage.title}
          </span>
        ) : (
          <>
            {choice.prompt.split(":")[0]} <span className="politics-panel__tag">{stage.title}</span>
          </>
        )}
      </div>
      <p className="text-muted">{stage.text}</p>
    </div>
  );
};

/** Inside an option row: the technology's colour, what it needs and its printed text. */
export const TechnologyPickOptionNote: React.FC<{ choice: PendingChoiceDto; option: ChoiceOption }> = ({
  choice,
  option,
}) => {
  const info = describeTechnologyPickOption(choice, option);
  if (!info) return null;
  return (
    <div data-testid="technology-pick-note" style={{ fontSize: 12 }}>
      {info.holders.length > 0 && (
        <div className="text-muted" data-testid="technology-pick-holders">
          Held by{" "}
          {info.holders.map((seat, index) => (
            <React.Fragment key={seat}>
              {index > 0 ? ", " : ""}
              <SeatTag seat={seat} />
            </React.Fragment>
          ))}
        </div>
      )}
      {info.owner && (
        <div className="text-muted" data-testid="technology-pick-owner">
          Owned by <SeatTag seat={info.owner} /> · you gain it, they keep theirs
        </div>
      )}
      <div className="text-muted">
        {info.track && (
          <strong style={{ color: info.accentColor ?? undefined }}>{info.track}</strong>
        )}
        {info.prereqs.length > 0 && (
          <>
            {info.track ? " · " : ""}needs{" "}
            {info.prereqs.map((symbol, index) => (
              <span
                key={index}
                data-testid="technology-pick-prereq"
                style={{
                  background: symbol.bgColor,
                  color: symbol.color,
                  borderRadius: 3,
                  padding: "0 4px",
                  marginRight: 2,
                  fontWeight: 700,
                }}
              >
                {symbol.label}
              </span>
            ))}
          </>
        )}
      </div>
      {info.text.map((line) => (
        <p key={line} className="politics-panel__text" data-testid="technology-pick-text" style={{ margin: "2px 0" }}>
          {line}
        </p>
      ))}
    </div>
  );
};
