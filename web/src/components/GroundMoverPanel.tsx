import React from "react";
import type { GroundMoverView } from "../presentation/groundMover.ts";
import { UnitIcon } from "./UnitIcon.tsx";
import "./ActionCardPickers.css";

/**
 * Ghost Squad: the planets of the active system and the moves between them, each with the counts
 * before and after. One tap moves a whole stack; the card asks again until you are done.
 */
export const GroundMoverPanel: React.FC<{
  view: GroundMoverView;
  disabled?: boolean;
  onChoose: (optionId: string) => void;
}> = ({ view, disabled, onChoose }) => (
  <section className="mover" data-testid="ground-mover" aria-label="Move ground forces">
    <div className="politics-panel">
      <div className="politics-panel__title">
        Ghost Squad
        <span className="politics-panel__tag">move ground forces</span>
      </div>
      <p className="text-muted">
        Each move takes every ground force of one type from one planet to another planet you control in this system. The card asks again after each move; choose Done to stop.
      </p>
    </div>
    <ul className="mover__planets" aria-label="Your ground forces by planet">
      {view.planets.map((planet) => (
        <li key={planet.id} className="mover__planet" data-testid="mover-planet">
          <strong>{planet.name}</strong>
          <span className="text-muted">{planet.forces}</span>
        </li>
      ))}
    </ul>
    <ul className="mover__moves" aria-label="Moves available">
      {view.moves.map((move) => (
        <li key={move.optionId}>
          <button
            type="button"
            className="mover__move"
            data-testid="mover-move"
            disabled={disabled}
            onClick={() => onChoose(move.optionId)}
            aria-label={`Move ${move.count} ${move.unitName} from ${move.fromName} to ${move.toName}: ${move.fromName} ${move.fromBefore} to ${move.fromAfter}, ${move.toName} ${move.toBefore} to ${move.toAfter}`}
          >
            <span className="mover__what">
              <UnitIcon type={move.unit} size={20} />
              <strong>
                {move.count} {move.unitName}
              </strong>
            </span>
            <span className="mover__route">
              <span>
                {move.fromName} <span className="mover__counts">{move.fromBefore} → {move.fromAfter}</span>
              </span>
              <span aria-hidden="true" className="mover__arrow">
                ➜
              </span>
              <span>
                {move.toName} <span className="mover__counts">{move.toBefore} → {move.toAfter}</span>
              </span>
            </span>
          </button>
        </li>
      ))}
    </ul>
    {view.doneId && (
      <div className="mover__actions">
        <button
          type="button"
          className="button button--primary"
          data-testid="mover-done"
          disabled={disabled}
          onClick={() => onChoose(view.doneId as string)}
        >
          Done
        </button>
      </div>
    )}
  </section>
);
