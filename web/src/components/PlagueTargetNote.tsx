import React from "react";
import type { BoardView, PendingChoiceDto } from "../protocol/types.ts";
import { describePlagueTarget } from "../presentation/plagueTarget.ts";
import { SeatTag } from "./SeatTag.tsx";
import "./ActionCardPickers.css";

/** Under the chosen planet of Plague: how many infantry roll, whose they are and what to expect. */
export const PlagueTargetNote: React.FC<{
  choice: PendingChoiceDto;
  planetId: string;
  board?: BoardView | null;
}> = ({ choice, planetId, board }) => {
  const target = describePlagueTarget(choice, planetId, board);
  if (!target) return null;
  return (
    <p className="plague-note text-muted" data-testid="plague-target-note">
      {target.infantry === 0 ? (
        "No infantry stand here: Plague rolls nothing."
      ) : (
        <>
          {target.infantry} {target.infantry === 1 ? "infantry rolls" : "infantry roll"} a die each (
          {target.owners.map((owner, index) => (
            <React.Fragment key={owner}>
              {index > 0 && ", "}
              <SeatTag seat={owner} />
            </React.Fragment>
          ))}
          ); each 6 or better destroys one. About {target.expectedKills} expected.
        </>
      )}
    </p>
  );
};
