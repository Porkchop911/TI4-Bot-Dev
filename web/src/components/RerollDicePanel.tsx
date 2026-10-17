import React, { useState } from "react";
import { humanizeId } from "../protocol/contentCatalog.ts";
import { rerollOutcome, type RerollDie, type RerollView } from "../presentation/rerollDice.ts";
import "./ActionCardPickers.css";

const unitName = (unit: string): string => humanizeId(unit.replace(/\d+$/, (digits) => (digits === "2" ? " II" : digits)));

const dieLabel = (die: RerollDie, ticked: boolean): string => {
  const verdict = die.hit === null ? "" : die.hit ? ", hit" : ", miss";
  const needs = die.hitsOn === null ? "" : `, needs ${die.hitsOn} or more`;
  const bonus = die.bonus ? `, ${die.bonus > 0 ? "+" : ""}${die.bonus} applied` : "";
  return `${unitName(die.unit)} die ${die.index + 1}: shows ${die.face}${bonus}${needs}${verdict}${die.decided ? ", already decided" : ticked ? ", will be rerolled" : ", kept"}`;
};

/**
 * The roll a reroll card offers, as dice: every die with its face and whether it hits, ticked
 * dice are redrawn. Replaces one radio per die, which showed neither the faces nor the threshold.
 */
export const RerollDicePanel: React.FC<{
  view: RerollView;
  disabled?: boolean;
  onConfirm: (ticked: ReadonlySet<number>) => void;
}> = ({ view, disabled, onConfirm }) => {
  const [ticked, setTicked] = useState<ReadonlySet<number>>(new Set());
  const outcome = rerollOutcome(view, ticked);
  const hits = view.dice.some((die) => die.hit !== null);
  const toggle = (index: number) =>
    setTicked((before) => {
      const next = new Set(before);
      if (next.has(index)) next.delete(index);
      else next.add(index);
      return next;
    });
  const open = view.dice.filter((die) => !die.decided);
  return (
    <section className="reroll" data-testid="reroll-panel" aria-label="Dice to reroll">
      <div className="politics-panel">
        <div className="politics-panel__title">
          {view.source ?? "Reroll"}
          <span className="politics-panel__tag">reroll any number of dice</span>
        </div>
        <p className="text-muted">
          Tick the dice to roll again. A rerolled die is redrawn and the new result counts, even when it is worse.
        </p>
      </div>
      {view.groups.map((group, groupIndex) => (
        <div key={`${group.unit}-${groupIndex}`} role="group" aria-label={`${unitName(group.unit)} dice`} className="reroll__group">
          <div className="reroll__heading">
            <strong>{unitName(group.unit)}</strong>
            {group.hitsOn !== null && <span className="text-muted"> hits on {group.hitsOn}+</span>}
          </div>
          <div className="reroll__dice">
            {group.dice.map((die) => {
              const isTicked = ticked.has(die.index);
              return (
                <button
                  key={die.index}
                  type="button"
                  className="reroll__die"
                  data-testid="reroll-die"
                  data-hit={die.hit === null ? undefined : String(die.hit)}
                  data-ticked={isTicked ? "true" : undefined}
                  aria-pressed={isTicked}
                  aria-label={dieLabel(die, isTicked)}
                  disabled={disabled || die.decided}
                  onClick={() => toggle(die.index)}
                >
                  <span className="reroll__face">{die.face}</span>
                  {die.bonus !== 0 && (
                    <span className="reroll__bonus">
                      {die.bonus > 0 ? "+" : ""}
                      {die.bonus}
                    </span>
                  )}
                  <span className="reroll__verdict">{die.hit === null ? "" : die.hit ? "Hit" : "Miss"}</span>
                  <span className="reroll__action">{isTicked ? "↻ reroll" : die.rerolled ? "rerolled" : "keep"}</span>
                </button>
              );
            })}
          </div>
        </div>
      ))}
      {hits && (
        <p className="reroll__summary" data-testid="reroll-summary">
          {outcome.now} {outcome.now === 1 ? "hit" : "hits"} now.{" "}
          {ticked.size === 0
            ? "Nothing ticked: the roll stands."
            : `Rerolling ${ticked.size} ${ticked.size === 1 ? "die" : "dice"}: about ${outcome.expected} ${outcome.expected === 1 ? "hit" : "hits"} expected (between ${outcome.worst} and ${outcome.best}).`}
        </p>
      )}
      <div className="reroll__actions">
        <button
          type="button"
          className="button button--primary"
          data-testid="reroll-confirm"
          disabled={disabled || open.length === 0}
          onClick={() => onConfirm(ticked)}
        >
          {ticked.size === 0 ? "Keep all dice" : `Reroll ${ticked.size} ${ticked.size === 1 ? "die" : "dice"}`}
        </button>
      </div>
    </section>
  );
};
