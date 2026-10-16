import React from "react";
import type { BoardView } from "../protocol/types.ts";
import type { ReachState } from "../hooks/useReachQuery.ts";
import { groupReach, type ReachEntry } from "../presentation/reachView.ts";
import { systemTag } from "../presentation/systemFacts.ts";
import { SeatBadge, usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { UnitIcon, getUnitBaseType, getUnitDisplayName } from "./UnitIcon.tsx";
import { VECTOR_COLORS } from "./board/MovementVectorsOverlay.tsx";

export interface SystemReachTabProps {
  systemId: string;
  board: BoardView;
  viewerSeat: string | null;
  state: ReachState;
  focusKey: string | null;
  onFocus: (key: string | null) => void;
}

/**
 * "Who can go here": the ships that could move into the selected system, asked of the engine's own
 * movement search and split by the viewer's own ships and everybody else's. It says what it assumes
 * (as if activated now, turn order ignored) because the real activation has more conditions.
 */
export const SystemReachTab: React.FC<SystemReachTabProps> = ({
  systemId,
  board,
  viewerSeat,
  state,
  focusKey,
  onFocus,
}) => {
  const groups =
    state.status === "ready" ? groupReach(state.origins, viewerSeat) : { own: [], enemy: [] };
  const total = groups.own.length + groups.enemy.length;
  return (
    <div data-testid="reach-tab" style={{ display: "flex", flexDirection: "column", gap: 10 }}>
      <p
        data-testid="reach-assumption"
        style={{ margin: 0, fontSize: 11, color: "#94a3b8", lineHeight: 1.4 }}
      >
        If {systemTag(systemId, board)} were activated now: every seat is asked as if it were its
        turn, ignoring tactic tokens and whose turn it is. Ships in a system with their owner&apos;s
        own command token cannot leave it and are not listed. Capacity and cargo are not checked.
      </p>
      <div
        data-testid="reach-legend"
        style={{ display: "flex", gap: 14, fontSize: 11, color: "#cbd5e1", flexWrap: "wrap" }}
      >
        <LegendSwatch color={VECTOR_COLORS.own} dash="8 5" label="Your ships" />
        <LegendSwatch color={VECTOR_COLORS.enemy} dash="3 6" label="Other players' ships" />
      </div>
      {state.status === "loading" && (
        <div data-testid="reach-loading" style={{ color: "#64748b", fontSize: 12 }}>
          Asking the engine...
        </div>
      )}
      {state.status === "unsupported" && (
        <div data-testid="reach-unsupported" style={{ color: "#fbbf24", fontSize: 12 }}>
          This server is too old to answer this. Update the server to use this tab.
        </div>
      )}
      {state.status === "error" && (
        <div data-testid="reach-error" style={{ color: "#fca5a5", fontSize: 12 }}>
          Could not get the answer right now. Select the system again to retry.
        </div>
      )}
      {state.status === "ready" && total === 0 && (
        <div data-testid="reach-empty" style={{ color: "#64748b", fontSize: 12 }}>
          No ship can move into this system right now.
        </div>
      )}
      {state.status === "ready" && total > 0 && (
        <>
          {viewerSeat !== null && (
            <ReachSection
              title="Your ships"
              testId="reach-own"
              entries={groups.own}
              board={board}
              focusKey={focusKey}
              onFocus={onFocus}
              emptyText="None of your ships can move here."
            />
          )}
          <ReachSection
            title={viewerSeat !== null ? "Other players' ships" : "Ships"}
            testId="reach-enemy"
            entries={groups.enemy}
            board={board}
            focusKey={focusKey}
            onFocus={onFocus}
            emptyText="No other player's ship can move here."
          />
        </>
      )}
    </div>
  );
};

const LegendSwatch: React.FC<{ color: string; dash: string; label: string }> = ({
  color,
  dash,
  label,
}) => (
  <span style={{ display: "inline-flex", alignItems: "center", gap: 6 }}>
    <svg width="28" height="8" aria-hidden="true">
      <line x1="1" y1="4" x2="27" y2="4" stroke={color} strokeWidth="3" strokeDasharray={dash} />
    </svg>
    {label}
  </span>
);

const ReachSection: React.FC<{
  title: string;
  testId: string;
  entries: ReachEntry[];
  board: BoardView;
  focusKey: string | null;
  onFocus: (key: string | null) => void;
  emptyText: string;
}> = ({ title, testId, entries, board, focusKey, onFocus, emptyText }) => {
  const display = usePlayerIdentity();
  return (
    <section data-testid={testId}>
      <h4
        style={{
          margin: "0 0 6px 0",
          fontSize: 12,
          fontWeight: 700,
          color: "#94a3b8",
          textTransform: "uppercase",
        }}
      >
        {title}
      </h4>
      {entries.length === 0 ? (
        <div style={{ color: "#64748b", fontSize: 11 }}>{emptyText}</div>
      ) : (
        <div style={{ display: "flex", flexDirection: "column", gap: 6 }}>
          {entries.map((entry) => {
            const who = display(entry.owner);
            const selected = focusKey === entry.key;
            return (
              <button
                key={entry.key}
                type="button"
                data-testid="reach-entry"
                data-owner={entry.owner}
                data-origin={entry.origin}
                aria-pressed={selected}
                onClick={() => onFocus(selected ? null : entry.key)}
                style={{
                  textAlign: "left",
                  minHeight: 44,
                  background: selected ? "rgba(56, 189, 248, 0.15)" : "rgba(15, 23, 42, 0.6)",
                  border: `1px solid ${selected ? "#38bdf8" : "rgba(51, 65, 85, 0.7)"}`,
                  borderLeft: `3px solid ${who.color}`,
                  borderRadius: 4,
                  padding: "6px 8px",
                  color: "#f8fafc",
                  cursor: "pointer",
                  font: "inherit",
                }}
              >
                <div
                  style={{ display: "flex", alignItems: "center", gap: 6, fontSize: 11, fontWeight: 600 }}
                >
                  {who.position && <SeatBadge position={who.position} />}
                  <span>{who.label}</span>
                  <span style={{ color: "#94a3b8", fontWeight: 400 }}>
                    from {systemTag(entry.origin, board)}
                  </span>
                </div>
                <div style={{ display: "flex", flexWrap: "wrap", gap: 4, marginTop: 4 }}>
                  {entry.ships.map((ship) => {
                    const base = getUnitBaseType(ship.type_id);
                    const boosts = [
                      ship.gravity_drive > 0 ? `${ship.gravity_drive} with Gravity Drive` : "",
                      ship.ionian > 0 ? `${ship.ionian} with Ionian Fuel Refinery` : "",
                    ].filter(Boolean);
                    return (
                      <span
                        key={ship.type_id}
                        data-testid="reach-ship"
                        style={{
                          display: "inline-flex",
                          alignItems: "center",
                          gap: 5,
                          padding: "2px 6px",
                          borderRadius: 4,
                          fontSize: 11,
                          background: "#1e293b",
                          color: "#e2e8f0",
                        }}
                      >
                        <UnitIcon type={base} size={14} color={who.color} aria-hidden="true" />
                        <span>
                          {ship.count} × {getUnitDisplayName(base, ship.count)}
                        </span>
                        {boosts.length > 0 && (
                          <span style={{ fontSize: 10, color: "#fbbf24" }}>({boosts.join(", ")})</span>
                        )}
                      </span>
                    );
                  })}
                </div>
              </button>
            );
          })}
        </div>
      )}
    </section>
  );
};
