import React, { createContext, useContext, useEffect, useState } from "react";
import { ReactionIntentStateMsg } from "../protocol/types.ts";
import { BLUFF_TRIGGERS, readDeclaredTriggers } from "../protocol/bluffTriggers.ts";

/** What the player mat needs to let this seat declare which reaction windows it bluffs about. */
export interface BluffControls {
  /** The server's answer for this seat, once it has given one. */
  intent: ReactionIntentStateMsg | null;
  /** The round now, for the "can change again in round N" lock. */
  round: number;
  /** Declares the whole set (empty clears). */
  onChange: (triggers: string[]) => void;
  /** What this browser had declared before the server answered. */
  stored: string[];
}

export const BluffContext = createContext<BluffControls | null>(null);

/** Provides the controls to the player mat; spectators get no provider and so no selector. */
export const BluffProvider: React.FC<{
  gameId: string;
  seat: string;
  intent: ReactionIntentStateMsg | null;
  round: number;
  onChange: (triggers: string[]) => void;
  children: React.ReactNode;
}> = ({ gameId, seat, intent, round, onChange, children }) => {
  const [stored] = useState(() => readDeclaredTriggers(gameId, seat));
  return (
    <BluffContext.Provider value={{ intent, round, onChange, stored }}>
      {children}
    </BluffContext.Provider>
  );
};

export function useBluffControls(): BluffControls | null {
  return useContext(BluffContext);
}

export interface BluffSelectorProps {
  controls: BluffControls;
  /** Public hand size; a seat with none cannot credibly bluff. */
  cardsCount: number;
  /** The seat set some card to Never offer to speed the game up. */
  neverMode: boolean;
}

/** Why the selector is off, in the words of the tooltip; null when it is usable. */
export function bluffDisabledReason(
  controls: Pick<BluffControls, "intent" | "round">,
  cardsCount: number,
  neverMode: boolean,
): string | null {
  if (cardsCount === 0)
    return "Hand size is public: with no action cards you cannot bluff one.";
  if (neverMode)
    return "You set a card to Never offer to speed the game up, so you cannot also stall it.";
  const { intent } = controls;
  if (intent && !intent.eligible && intent.ineligible_reason)
    return `You cannot bluff: ${intent.ineligible_reason}.`;
  return null;
}

export function bluffLockedReason(
  controls: Pick<BluffControls, "intent" | "round">,
): string | null {
  const until = controls.intent?.locked_until_round;
  if (until !== undefined && controls.round < until)
    return `Locked: your bluffed reactions can change again in round ${until}.`;
  return null;
}

export const BluffSelector: React.FC<BluffSelectorProps> = ({ controls, cardsCount, neverMode }) => {
  const { intent, onChange } = controls;
  const serverSet = intent ? intent.triggers : controls.stored;
  // The picks being made; nothing is sent until "Declare", because a declaration can change only
  // once per round and a single pick must not use that up.
  const [draft, setDraft] = useState<string[] | null>(null);
  // The server's answer replaces the draft.
  useEffect(() => setDraft(null), [intent]);
  const declared = draft ?? serverSet;
  const dirty =
    draft !== null &&
    (draft.length !== serverSet.length || draft.some((id) => !serverSet.includes(id)));
  const max = intent?.max_triggers ?? 3;
  const off = bluffDisabledReason(controls, cardsCount, neverMode);
  const locked = bluffLockedReason(controls);
  const frozen = off !== null || locked !== null;
  const pick = (next: string[]) => setDraft(next);
  return (
    <details
      data-testid="bluff-selector"
      data-state={off ? "disabled" : locked ? "locked" : "open"}
      style={{ fontSize: 12 }}
      open={declared.length > 0 || undefined}
    >
      <summary
        title={off ?? locked ?? "Pick the moments you want to look like you may react to"}
        style={{ cursor: "pointer", color: off ? "#64748b" : "#38bdf8", fontWeight: "bold" }}
      >
        Bluff a reaction{declared.length > 0 ? ` (${declared.length} declared)` : ""}
      </summary>
      <div style={{ display: "grid", gap: 4, marginTop: 6 }}>
        <div style={{ color: "#94a3b8", fontSize: 11, lineHeight: 1.3 }}>
          When one of these moments comes up and you are not asked anything, the game waits a few
          seconds as if you were thinking. Only you can see what you picked.
        </div>
        {off && (
          <div data-testid="bluff-disabled-reason" style={{ color: "#fbbf24", fontSize: 11 }}>
            {off}
          </div>
        )}
        {!off && locked && (
          <div data-testid="bluff-locked-reason" style={{ color: "#fbbf24", fontSize: 11 }}>
            {locked}
          </div>
        )}
        {!off && intent?.budget_used_up && (
          <div data-testid="bluff-budget-used-up" style={{ color: "#fbbf24", fontSize: 11 }}>
            Bluff budget used up: no more waits this round or game.
          </div>
        )}
        {BLUFF_TRIGGERS.map((trigger) => {
          const checked = declared.includes(trigger.id);
          const full = !checked && declared.length >= max;
          const disabled = frozen || full;
          return (
            <label
              key={trigger.id}
              title={
                off ?? locked ?? (full ? `You can pick at most ${max} moments` : trigger.label)
              }
              style={{
                display: "flex",
                alignItems: "center",
                gap: 6,
                color: disabled ? "#64748b" : "#e2e8f0",
                cursor: disabled ? "not-allowed" : "pointer",
              }}
            >
              <input
                type="checkbox"
                data-testid={`bluff-trigger-${trigger.id}`}
                checked={checked}
                disabled={disabled}
                onChange={() =>
                  pick(
                    checked
                      ? declared.filter((id) => id !== trigger.id)
                      : [...declared, trigger.id],
                  )
                }
              />
              {trigger.label}
            </label>
          );
        })}
        <div style={{ display: "flex", gap: 8 }}>
          <button
            type="button"
            className="button button--primary"
            data-testid="bluff-declare"
            disabled={frozen || !dirty || declared.length === 0}
            title={
              frozen
                ? (off ?? locked ?? undefined)
                : "Declare these moments. You can change them again next round."
            }
            onClick={() => onChange(declared)}
          >
            Declare
          </button>
          {serverSet.length > 0 && (
            <button
              type="button"
              className="button button--secondary"
              data-testid="bluff-clear"
              // Clearing is always allowed, even while the declaration is locked.
              onClick={() => onChange([])}
            >
              Clear
            </button>
          )}
        </div>
      </div>
    </details>
  );
};

/**
 * Shown to the bluffer only while a hold is running: the other players see the ordinary
 * "Waiting for ..." status, and this seat can end the wait early.
 */
export const BluffHoldBar: React.FC<{ holding: boolean; onPass: () => void }> = ({
  holding,
  onPass,
}) => {
  if (!holding) return null;
  return (
    <div
      data-testid="bluff-hold-bar"
      role="status"
      style={{
        display: "flex",
        alignItems: "center",
        gap: 12,
        padding: "8px 20px",
        background: "#1e1b4b",
        borderBottom: "1px solid #4338ca",
        color: "#e0e7ff",
        fontSize: 13,
      }}
    >
      <span>Bluffing: the table is waiting for you. Pass to end it early.</span>
      <button
        type="button"
        className="button button--secondary"
        data-testid="bluff-pass"
        onClick={onPass}
      >
        Pass
      </button>
    </div>
  );
};
