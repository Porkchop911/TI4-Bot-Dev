import React, { createContext, useContext } from "react";
import type { ChoiceOption, PendingChoiceDto } from "../protocol/types.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import {
  describeAgendaPlacement,
  describeDecisionAgenda,
  describePickCard,
  optionNote,
  replenishReason,
  seatStanding,
  type DecisionTable,
} from "../presentation/politicsDecision.ts";

const TableContext = createContext<DecisionTable | null>(null);

/** The table standing the surrounding provider holds, or `null` outside one. */
export const useDecisionTable = (): DecisionTable | null => useContext(TableContext);

/** Gives decisions the table standing (VP, commodities, speaker order) the client already holds. */
export const DecisionTableProvider: React.FC<{
  table: DecisionTable | null;
  children: React.ReactNode;
}> = ({ table, children }) => <TableContext.Provider value={table}>{children}</TableContext.Provider>;

/** Above the options: the agenda's printed card, or the reason line of the Hacan Agent choice. */
export const PoliticsContextPanel: React.FC<{ choice: PendingChoiceDto }> = ({ choice }) => {
  const table = useContext(TableContext);
  const agenda = describeAgendaPlacement(choice);
  const reason = replenishReason(choice, table);
  const pickCard = describePickCard(choice);
  const about = describeDecisionAgenda(choice);
  if (!agenda && !reason && !pickCard && !about) return null;
  return (
    <div className="politics-panel" data-testid="politics-context-panel">
      {pickCard && (
        <div data-testid="pick-card">
          <div className="politics-panel__title" data-testid="pick-card-name">
            {pickCard.name}
            <span className="politics-panel__tag">pick a player</span>
          </div>
          {pickCard.text && (
            <p className="politics-panel__text" data-testid="pick-card-text">
              {pickCard.text}
            </p>
          )}
        </div>
      )}
      {about && (
        <div data-testid="decision-agenda-card">
          <div className="text-muted politics-panel__eyebrow">{about.revealed ? "Revealed agenda" : "Agenda"}</div>
          <div className="politics-panel__title" data-testid="decision-agenda-name">
            {about.name}
            {about.kind && <span className="politics-panel__tag">{about.kind}</span>}
            {about.target && <span className="politics-panel__tag">{about.target}</span>}
          </div>
          {about.text.map((line) => (
            <p key={line} className="politics-panel__text" data-testid="decision-agenda-text">
              {line}
            </p>
          ))}
        </div>
      )}
      {agenda && (
        <div data-testid="agenda-card">
          <div className="politics-panel__title" data-testid="agenda-card-name">
            {agenda.name}
            {agenda.kind && <span className="politics-panel__tag">{agenda.kind}</span>}
          </div>
          {agenda.target && <div className="text-muted">Elects: {agenda.target}</div>}
          {agenda.text.map((line) => (
            <p key={line} className="politics-panel__text" data-testid="agenda-card-text">
              {line}
            </p>
          ))}
          <p className="text-muted">Top: it is revealed next. Bottom: it will not come up soon.</p>
        </div>
      )}
      {reason && (
        <p className="politics-panel__reason" data-testid="hacan-reason">
          {reason}
        </p>
      )}
    </div>
  );
};

/** A line under an option's label: who it is (faction, seat) and what choosing it does. */
export const PoliticsOptionNote: React.FC<{ choice: PendingChoiceDto; option: ChoiceOption }> = ({
  choice,
  option,
}) => {
  const table = useContext(TableContext);
  const display = usePlayerIdentity();
  const note = optionNote(choice, option, table);
  if (!note) return null;
  const standing = table ? seatStanding(table, note.seat) : null;
  const who = note.seat ? display(note.seat) : null;
  return (
    <div className="politics-option-note" data-testid="choice-option-note">
      {who && standing && (
        <strong className="politics-option-note__who" style={{ color: who.color }}>
          {who.symbol} {standing.faction}
          {who.position ? ` (seat ${who.position})` : ""}:{" "}
        </strong>
      )}
      <span>{note.text}</span>
    </div>
  );
};
