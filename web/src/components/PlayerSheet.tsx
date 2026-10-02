import React from "react";
import { BoardView, PlayerView } from "../protocol/types.ts";
import {
  getStrategyCardMeta,
  getSecretObjectiveMeta,
  getActionCardMeta,
} from "../protocol/contentCatalog.ts";
import { CardSubject } from "./CardDetails.tsx";
import { SeatBadge, usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { computePlayerStats } from "../presentation/playerStats.ts";
import { Tooltip } from "../primitives/index.ts";

const ResourceIcon: React.FC<{ style?: React.CSSProperties }> = ({ style }) => (
  <svg
    viewBox="0 0 16 16"
    width="13"
    height="13"
    fill="currentColor"
    aria-hidden="true"
    style={{ display: "inline-block", verticalAlign: "-2px", color: "#fbbf24", ...style }}
  >
    <path d="M8 1 L14 7 L8 15 L2 7 Z" />
  </svg>
);

const InfluenceIcon: React.FC<{ style?: React.CSSProperties }> = ({ style }) => (
  <svg
    viewBox="0 0 16 16"
    width="13"
    height="13"
    fill="currentColor"
    aria-hidden="true"
    style={{ display: "inline-block", verticalAlign: "-2px", color: "#38bdf8", ...style }}
  >
    <path d="M2 13h12v1.5H2zm1.5-2l2-6 2.5 3 2.5-3 2 6H3.5z" />
  </svg>
);

const SystemIcon: React.FC<{ style?: React.CSSProperties }> = ({ style }) => (
  <svg
    viewBox="0 0 16 16"
    width="13"
    height="13"
    fill="none"
    stroke="currentColor"
    strokeWidth="1.5"
    aria-hidden="true"
    style={{ display: "inline-block", verticalAlign: "-2px", color: "#a78bfa", ...style }}
  >
    <polygon points="8,1.5 14.5,5.2 14.5,10.8 8,14.5 1.5,10.8 1.5,5.2" />
  </svg>
);

const PlanetIcon: React.FC<{ style?: React.CSSProperties }> = ({ style }) => (
  <svg
    viewBox="0 0 16 16"
    width="13"
    height="13"
    fill="currentColor"
    aria-hidden="true"
    style={{ display: "inline-block", verticalAlign: "-2px", color: "#34d399", ...style }}
  >
    <circle cx="8" cy="8" r="4.5" />
    <path d="M1 8.5c1.5-3 6.5-4 12-2.5l.5-.8C7.5 3.5 2 4.8.5 7.8l.5.7zm14-1c-1.5 3-6.5 4-12 2.5l-.5.8c6 1.7 11.5.4 13-2.6l-.5-.7z" />
  </svg>
);

const ProductionIcon: React.FC<{ style?: React.CSSProperties }> = ({ style }) => (
  <svg
    viewBox="0 0 16 16"
    width="13"
    height="13"
    fill="currentColor"
    aria-hidden="true"
    style={{ display: "inline-block", verticalAlign: "-2px", color: "#fb923c", ...style }}
  >
    <path d="M1 14h14v-1.5H1v1.5zm1-2.5h3V8.5l3 2v-3l4 3V4.5h2v7H2z" />
  </svg>
);

export interface PlayerSheetProps {
  players: PlayerView[];
  userSeat?: string;
  revealedObjectives?: string[];
  board?: BoardView;
  onInspectCard?: (subject: CardSubject) => void;
}

export const PlayerSheet: React.FC<PlayerSheetProps> = ({
  players,
  userSeat,
  revealedObjectives: _revealedObjectives = [],
  board,
  onInspectCard,
}) => {
  const display = usePlayerIdentity();
  return (
    <aside
      data-testid="player-sheet-panel"
      className="player-sheet-panel"
      style={{
        width: 330,
        height: "100%",
        borderLeft: "1px solid var(--color-border)",
        overflowY: "auto",
        padding: 16,
        display: "flex",
        flexDirection: "column",
        gap: 16,
      }}
    >
      <h2 style={{ fontSize: 16, fontWeight: "bold", margin: 0, color: "#94a3b8" }}>Players</h2>

      {players.map((player) => {
        const isSelf = userSeat === player.id;
        const identity = display(player.id);
        const stats = computePlayerStats(player, board);

        return (
          <div
            key={player.id}
            data-testid="player-card"
            data-is-self={isSelf ? "true" : "false"}
            className={`card${isSelf ? " card--selected" : ""}`}
            style={{
              padding: 14,
              borderLeft: `4px solid ${identity.color}`,
              display: "flex",
              flexDirection: "column",
              gap: 10,
            }}
          >
            {/* Header: current name + physical position + faction + VP */}
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
              <div>
                <strong style={{ fontSize: 15, color: "#f8fafc" }}>
                  {identity.position && <SeatBadge position={identity.position} />} {identity.label}{" "}
                  {isSelf && "(You)"}
                </strong>
                <div style={{ fontSize: 12, color: "#94a3b8" }}>{player.faction}</div>
              </div>
              <div
                data-testid="player-vp"
                style={{
                  background: "#fbbf24",
                  color: "#0f172a",
                  fontWeight: "bold",
                  fontSize: 14,
                  padding: "2px 8px",
                  borderRadius: 12,
                }}
              >
                {player.victory_points} VP
              </div>
            </div>

            {/* Economy & Tokens */}
            <div
              style={{
                display: "grid",
                gridTemplateColumns: "1fr 1fr",
                gap: 6,
                fontSize: 12,
                color: "#cbd5e1",
              }}
            >
              <div>
                TG: <strong>{player.trade_goods}</strong> | Comm:{" "}
                <strong>{player.commodities}</strong>
              </div>
              <div>
                Tokens:{" "}
                <strong>
                  {player.tactic_tokens}/{player.fleet_tokens}/{player.strategic_tokens}
                </strong>
              </div>
            </div>

            {/* Additional Player Stats (Resources, Influence, Systems, Planets, Production) */}
            <div
              data-testid="player-stats-row"
              className="player-stats-row"
              style={{
                display: "flex",
                flexWrap: "wrap",
                gap: "4px 8px",
                alignItems: "center",
                padding: "4px 8px",
                backgroundColor: "rgba(15, 23, 42, 0.6)",
                border: "1px solid rgba(255, 255, 255, 0.08)",
                borderRadius: 6,
                fontSize: 12,
              }}
            >
              {/* Remaining / Total Resources */}
              <Tooltip
                content={`Resources: ${stats.remainingResources} ready / ${stats.totalResources} total`}
              >
                <div
                  data-testid="player-resources"
                  title={`Resources: ${stats.remainingResources} ready / ${stats.totalResources} total`}
                  tabIndex={0}
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 3,
                    cursor: "default",
                  }}
                >
                  <ResourceIcon />
                  <span style={{ fontWeight: 600 }}>
                    <span style={{ color: stats.remainingResources > 0 ? "#f8fafc" : "#94a3b8" }}>
                      {stats.remainingResources}
                    </span>
                    <span style={{ color: "#64748b" }}>/</span>
                    <span style={{ color: "#94a3b8" }}>{stats.totalResources}</span>
                  </span>
                </div>
              </Tooltip>

              {/* Remaining / Total Influence */}
              <Tooltip
                content={`Influence: ${stats.remainingInfluence} ready / ${stats.totalInfluence} total`}
              >
                <div
                  data-testid="player-influence"
                  title={`Influence: ${stats.remainingInfluence} ready / ${stats.totalInfluence} total`}
                  tabIndex={0}
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 3,
                    cursor: "default",
                  }}
                >
                  <InfluenceIcon />
                  <span style={{ fontWeight: 600 }}>
                    <span style={{ color: stats.remainingInfluence > 0 ? "#f8fafc" : "#94a3b8" }}>
                      {stats.remainingInfluence}
                    </span>
                    <span style={{ color: "#64748b" }}>/</span>
                    <span style={{ color: "#94a3b8" }}>{stats.totalInfluence}</span>
                  </span>
                </div>
              </Tooltip>

              {/* Controlled Systems */}
              <Tooltip content={`Controlled Systems: ${stats.controlledSystems}`}>
                <div
                  data-testid="player-controlled-systems"
                  title={`Controlled Systems: ${stats.controlledSystems}`}
                  tabIndex={0}
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 3,
                    cursor: "default",
                  }}
                >
                  <SystemIcon />
                  <strong style={{ color: "#f8fafc" }}>{stats.controlledSystems}</strong>
                </div>
              </Tooltip>

              {/* Controlled Planets */}
              <Tooltip content={`Controlled Planets: ${stats.controlledPlanets}`}>
                <div
                  data-testid="player-controlled-planets"
                  title={`Controlled Planets: ${stats.controlledPlanets}`}
                  tabIndex={0}
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 3,
                    cursor: "default",
                  }}
                >
                  <PlanetIcon />
                  <strong style={{ color: "#f8fafc" }}>{stats.controlledPlanets}</strong>
                </div>
              </Tooltip>

              {/* Available / Total Production Capacity */}
              <Tooltip
                content={`Production Capacity: ${stats.availableProductionCapacity} available / ${stats.totalProductionCapacity} total`}
              >
                <div
                  data-testid="player-production-capacity"
                  title={`Production Capacity: ${stats.availableProductionCapacity} available / ${stats.totalProductionCapacity} total`}
                  tabIndex={0}
                  style={{
                    display: "inline-flex",
                    alignItems: "center",
                    gap: 3,
                    cursor: "default",
                  }}
                >
                  <ProductionIcon />
                  <span style={{ fontWeight: 600 }}>
                    <span
                      style={{
                        color: stats.availableProductionCapacity > 0 ? "#f8fafc" : "#94a3b8",
                      }}
                    >
                      {stats.availableProductionCapacity}
                    </span>
                    <span style={{ color: "#64748b" }}>/</span>
                    <span style={{ color: "#94a3b8" }}>{stats.totalProductionCapacity}</span>
                  </span>
                </div>
              </Tooltip>
            </div>

            {/* Public strategy cards */}
            {player.strategy_cards.length > 0 && (
              <div style={{ fontSize: 12 }}>
                <div style={{ color: "#94a3b8", marginBottom: 4, fontWeight: 500 }}>
                  Strategy Cards:
                </div>
                <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
                  {player.strategy_cards.map((scId) => {
                    const meta = getStrategyCardMeta(scId);
                    const tooltipText = `${meta.name} (Initiative ${meta.initiative})\n\nPrimary:\n${meta.primaryText}\n\nSecondary:\n${meta.secondaryText}`;
                    return (
                      <button
                        type="button"
                        key={scId}
                        onClick={() => onInspectCard?.({ kind: "strategy", id: scId })}
                        data-testid={`strategy-card-badge-${scId}`}
                        data-card-id={scId}
                        title={tooltipText}
                        style={{
                          background: "#090d16",
                          border: "1px solid #38bdf8",
                          borderRadius: 4,
                          padding: "2px 8px",
                          fontSize: 12,
                          fontWeight: 600,
                          color: "#38bdf8",
                          cursor: "pointer",
                          display: "inline-block",
                        }}
                      >
                        {meta.initiative > 0 ? `${meta.initiative}. ` : ""}
                        {meta.name}
                      </button>
                    );
                  })}
                </div>
              </div>
            )}

            {/* Public Card Counts */}
            <div style={{ display: "flex", gap: 10, fontSize: 11, color: "#94a3b8" }}>
              <span>
                Action Cards: <strong>{player.action_cards_count}</strong>
              </span>
              <span>
                Secret Obj: <strong>{player.secret_objectives_count}</strong>
              </span>
            </div>

            {player.scored_secret_objectives && player.scored_secret_objectives.length > 0 && (
              <div style={{ display: "grid", gap: 4 }}>
                <span className="text-muted">Scored Secret Objectives:</span>
                {player.scored_secret_objectives.map((id) => (
                  <button
                    type="button"
                    key={id}
                    className="button button--secondary detail-trigger"
                    data-testid={`scored-objective-${id}`}
                    onClick={() => onInspectCard?.({ kind: "secretObjective", id })}
                  >
                    {getSecretObjectiveMeta(id).name}
                  </button>
                ))}
              </div>
            )}

            {/* Private Information (Visible only to this player's seat) */}
            {isSelf && (
              <div
                data-testid="private-hand-section"
                style={{
                  marginTop: 6,
                  borderTop: "1px dashed #475569",
                  paddingTop: 10,
                  display: "flex",
                  flexDirection: "column",
                  gap: 10,
                }}
              >
                <div style={{ fontSize: 12, fontWeight: "bold", color: "#38bdf8" }}>
                  Private Hand
                </div>

                {/* Held Action Cards */}
                {player.held_action_cards && player.held_action_cards.length > 0 ? (
                  <div>
                    <div style={{ fontSize: 11, color: "#94a3b8", marginBottom: 4 }}>
                      Action Cards ({player.held_action_cards.length}):
                    </div>
                    <ul
                      style={{
                        margin: 0,
                        padding: 0,
                        listStyle: "none",
                        display: "flex",
                        flexDirection: "column",
                        gap: 6,
                      }}
                    >
                      {player.held_action_cards.map((cardId) => {
                        const meta = getActionCardMeta(cardId);
                        const tooltipText = `${meta.name} (${meta.phase ?? "Action"})\n\n${meta.description}`;

                        return (
                          <li
                            key={cardId}
                            data-private-card="true"
                            data-private-card-owner={
                              identity.position ? `position-${identity.position}` : "participant"
                            }
                            data-action-card-id={cardId}
                            data-testid={`action-card-item-${cardId}`}
                            title={tooltipText}
                            className="card"
                            style={{
                              border: "1px solid #475569",
                              padding: "6px 10px",
                              cursor: "pointer",
                              position: "relative",
                            }}
                          >
                            <button
                              type="button"
                              className="detail-trigger"
                              onClick={() => onInspectCard?.({ kind: "action", id: cardId })}
                            >
                              <div style={{ width: "100%" }}>
                                <div
                                  style={{
                                    display: "flex",
                                    justifyContent: "space-between",
                                    alignItems: "center",
                                  }}
                                >
                                  <strong style={{ color: "#e2e8f0", fontSize: 12 }}>
                                    {meta.name}
                                  </strong>
                                  {meta.phase && (
                                    <span
                                      style={{
                                        fontSize: 10,
                                        color: "#94a3b8",
                                        background: "#1e293b",
                                        padding: "1px 5px",
                                        borderRadius: 3,
                                      }}
                                    >
                                      {meta.phase}
                                    </span>
                                  )}
                                </div>
                                <div
                                  style={{
                                    fontSize: 11,
                                    color: "#94a3b8",
                                    marginTop: 2,
                                    lineHeight: 1.3,
                                  }}
                                >
                                  {meta.description}
                                </div>
                              </div>
                            </button>
                          </li>
                        );
                      })}
                    </ul>
                  </div>
                ) : (
                  <div style={{ fontSize: 11, color: "#64748b" }}>No action cards in hand</div>
                )}

                {/* Held Secret Objectives (Human Readable with Tooltips) */}
                {player.held_secret_objectives && player.held_secret_objectives.length > 0 && (
                  <div>
                    <div style={{ fontSize: 11, color: "#94a3b8", marginBottom: 4 }}>
                      Secret Objectives ({player.held_secret_objectives.length}):
                    </div>
                    <ul
                      style={{
                        margin: 0,
                        padding: 0,
                        listStyle: "none",
                        display: "flex",
                        flexDirection: "column",
                        gap: 6,
                      }}
                    >
                      {player.held_secret_objectives.map((objId) => {
                        const meta = getSecretObjectiveMeta(objId);
                        const tooltipText = `${meta.name} (${meta.phase} Phase, ${meta.points} VP)\n\nRequirement: ${meta.description}`;

                        return (
                          <li
                            key={objId}
                            data-private-card="true"
                            data-private-card-owner={
                              identity.position ? `position-${identity.position}` : "participant"
                            }
                            data-secret-obj-id={objId}
                            data-testid={`secret-objective-item-${objId}`}
                            title={tooltipText}
                            className="card"
                            style={{
                              border: "1px solid #fbbf24",
                              padding: "6px 10px",
                              cursor: "pointer",
                              position: "relative",
                            }}
                          >
                            <button
                              type="button"
                              className="detail-trigger"
                              onClick={() =>
                                onInspectCard?.({ kind: "secretObjective", id: objId })
                              }
                            >
                              <div style={{ width: "100%" }}>
                                <div
                                  style={{
                                    display: "flex",
                                    justifyContent: "space-between",
                                    alignItems: "center",
                                  }}
                                >
                                  <strong style={{ color: "#fbbf24", fontSize: 12 }}>
                                    {meta.name}
                                  </strong>
                                  <span
                                    style={{
                                      fontSize: 10,
                                      color: "#fbbf24",
                                      background: "rgba(251, 191, 36, 0.15)",
                                      padding: "1px 5px",
                                      borderRadius: 3,
                                    }}
                                  >
                                    {meta.phase} • {meta.points} VP
                                  </span>
                                </div>
                                <div
                                  style={{
                                    fontSize: 11,
                                    color: "#cbd5e1",
                                    marginTop: 3,
                                    lineHeight: 1.3,
                                  }}
                                >
                                  {meta.description}
                                </div>
                              </div>
                            </button>
                          </li>
                        );
                      })}
                    </ul>
                  </div>
                )}
              </div>
            )}
          </div>
        );
      })}
    </aside>
  );
};
