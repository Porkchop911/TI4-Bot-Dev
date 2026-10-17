import { ARMADA_BONUS, ARMADA_TEXT, hasArmada } from "../presentation/commandTokens.ts";
import React, { useState } from "react";
import { PlanetValue } from "./PlanetValueIcons.tsx";
import { BoardView, PlayerView, ReactionModes, ReactionModeSetting, TableView } from "../protocol/types.ts";
import { BluffSelector, useBluffControls } from "./BluffSelector.tsx";
import {
  getStrategyCardMeta,
  getSecretObjectiveMeta,
  getActionCardMeta,
  PUBLIC_OBJECTIVES,
  SECRET_OBJECTIVES,
} from "../protocol/contentCatalog.ts";
import { CardSubject } from "./CardDetails.tsx";
import { FactionInfoButton } from "./UnitInfo.tsx";
import { describeBreakthrough } from "../presentation/breakthroughs.ts";
import { usePublicState } from "../presentation/PublicStateContext.tsx";
import { SecondaryPrepModeToggle } from "./SecondaryPrepModeToggle.tsx";
import { SeatBadge, usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { computePlayerStats } from "../presentation/playerStats.ts";
import { Tooltip } from "../primitives/index.ts";
import { useTurnSound } from "../hooks/useTurnSound.ts";
import { useToastMute } from "../hooks/useToastMute.ts";
import { useTurnRecapSetting } from "../hooks/useTurnRecapSetting.ts";
import { useAutoSubmitSetting } from "../hooks/useAutoSubmitSetting.ts";
import { SecretObjectiveProgress } from "./SecretObjectiveProgress.tsx";
import { secretProgressOf } from "../presentation/secretProgress.ts";


/**
 * "Holds: [owner] [owner]": one chip per foreign command token in the fleet pool, in the owner's
 * seat colour. They count toward the fleet supply but are not part of the sheet's fleet count.
 */
export const HeldForeignTokens: React.FC<{ owners: readonly string[] }> = ({ owners }) => {
  const display = usePlayerIdentity();
  const names = owners.map((owner) => display(owner).label).join(", ");
  const summary = `Holds ${owners.length} other ${owners.length === 1 ? "player's" : "players'"} command ${
    owners.length === 1 ? "token" : "tokens"
  } in the fleet pool (${names}). They raise the fleet supply and cannot be redistributed.`;
  return (
    <div
      data-testid="player-held-tokens"
      aria-label={summary}
      title={summary}
      style={{
        display: "flex",
        flexWrap: "wrap",
        alignItems: "center",
        gap: "3px 6px",
        fontSize: 12,
        color: "#cbd5e1",
      }}
    >
      <span>Holds:</span>
      {owners.map((owner, index) => {
        const identity = display(owner);
        return (
          <span
            key={`${owner}-${index}`}
            data-testid="player-held-token"
            style={{
              display: "inline-flex",
              alignItems: "center",
              gap: 4,
              padding: "1px 6px",
              borderRadius: 10,
              border: `1px solid ${identity.color}`,
              background: "rgba(15, 23, 42, 0.6)",
            }}
          >
            <span aria-hidden="true" style={{ color: identity.color }}>
              ◆
            </span>
            {identity.label}
          </span>
        );
      })}
    </div>
  );
};

export interface VPBreakdown {
  publicObjectives: number;
  publicVP: number;
  secretObjectives: number;
  secretVP: number;
  /** Points the client cannot attribute (custodians, Imperial, relics, laws, abilities). */
  otherVP: number;
  /** Server-authoritative total (player.victory_points). */
  total: number;
  /** True when the known objective points exceed the server total. */
  knownExceedsTotal: boolean;
}

export const calculateVPBreakdown = (
  player: PlayerView,
  _board?: BoardView,
  table?: TableView
): VPBreakdown => {
  let publicObjectives = 0;
  let publicVP = 0;
  let secretObjectives = 0;
  let secretVP = 0;

  for (const objId of table?.scored_objectives?.[player.id] ?? []) {
    const objMeta = PUBLIC_OBJECTIVES[objId as keyof typeof PUBLIC_OBJECTIVES];
    if (objMeta) {
      publicObjectives++;
      publicVP += objMeta.points;
    }
  }

  for (const objId of player.scored_secret_objectives ?? []) {
    const objMeta = SECRET_OBJECTIVES[objId as keyof typeof SECRET_OBJECTIVES];
    if (objMeta) {
      secretObjectives++;
      secretVP += objMeta.points;
    }
  }

  const total = Math.max(0, player.victory_points);
  const known = publicVP + secretVP;
  const knownExceedsTotal = known > total;
  if (knownExceedsTotal) {
    // Never show more than the server total: clamp the known lines to it.
    const cappedPublic = Math.min(publicVP, total);
    secretVP = Math.min(secretVP, total - cappedPublic);
    publicVP = cappedPublic;
  }
  return {
    publicObjectives,
    publicVP,
    secretObjectives,
    secretVP,
    otherVP: Math.max(0, total - publicVP - secretVP),
    total,
    knownExceedsTotal,
  };
};

export const VPBreakdownTooltip: React.FC<{ breakdown: VPBreakdown }> = ({ breakdown }) => (
  <div className="vp-breakdown-tooltip">
    <div className="vp-breakdown-row">
      <span className="vp-breakdown-label">Public Objectives:</span>
      <span className="vp-breakdown-count">{breakdown.publicObjectives}</span>
      <span className="vp-breakdown-vp">+{breakdown.publicVP} VP</span>
    </div>
    <div className="vp-breakdown-row">
      <span className="vp-breakdown-label">Secret Objectives:</span>
      <span className="vp-breakdown-count">{breakdown.secretObjectives}</span>
      <span className="vp-breakdown-vp">+{breakdown.secretVP} VP</span>
    </div>
    <div className="vp-breakdown-row" data-testid="vp-breakdown-other">
      <span className="vp-breakdown-label">Other sources (custodians, Imperial, relics, laws, abilities):</span>
      <span className="vp-breakdown-vp">+{breakdown.otherVP} VP</span>
    </div>
    <div className="vp-breakdown-row" data-testid="vp-breakdown-total">
      <span className="vp-breakdown-label">Total:</span>
      <span className="vp-breakdown-vp">{breakdown.total} VP</span>
    </div>
    {breakdown.knownExceedsTotal && (
      <div className="vp-breakdown-note">
        Scored objectives exceed the reported total; showing the server total.
      </div>
    )}
  </div>
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
  seatingOrder?: string[];
  revealedObjectives?: string[];
  board?: BoardView;
  table?: TableView;
  onInspectCard?: (subject: CardSubject) => void;
  /** The viewing seat's "never offer" cards by printed name, as the server holds them. */
  reactionModes?: ReactionModes;
  /** Changes one card's mode on the server; without it (a spectator) the toggle is not shown. */
  onSetReactionMode?: (card: string, mode: ReactionModeSetting) => void;
}

/**
 * A seat's Thunder's Edge breakthrough: unlocked (printed text in full) or still locked (name only,
 * the text on hover; the card is public). Nothing shows for a faction with no printed breakthrough.
 */
const BreakthroughRow: React.FC<{ faction: string; held: string | null }> = ({ faction, held }) => {
  const line = describeBreakthrough(faction, held);
  if (!line) return null;
  const detail = `${line.name} (synergy ${line.synergy})${line.text ? `\n\n${line.text}` : ""}`;
  return (
    <div
      data-testid="player-breakthrough"
      data-unlocked={line.unlocked ? "true" : "false"}
      title={detail}
      style={{
        fontSize: 11,
        padding: "3px 6px",
        borderRadius: 4,
        border: `1px solid ${line.unlocked ? "#f59e0b" : "rgba(255, 255, 255, 0.12)"}`,
        background: line.unlocked ? "rgba(245, 158, 11, 0.1)" : "transparent",
        color: line.unlocked ? "#fde68a" : "#94a3b8",
      }}
    >
      <div style={{ display: "flex", flexWrap: "wrap", gap: 6, alignItems: "baseline" }}>
        <span style={{ fontWeight: 600 }}>Breakthrough:</span>
        <strong data-testid="player-breakthrough-name">{line.name}</strong>
        <span data-testid="player-breakthrough-state">{line.unlocked ? "unlocked" : "locked"}</span>
        <span>Synergy: {line.synergy}</span>
      </div>
      {line.unlocked && line.text && (
        <div data-testid="player-breakthrough-text" style={{ marginTop: 3, whiteSpace: "pre-line", color: "#e2e8f0" }}>
          {line.text}
        </div>
      )}
    </div>
  );
};

export const PlayerSheet: React.FC<PlayerSheetProps> = ({
  players,
  userSeat,
  seatingOrder = [],
  revealedObjectives: _revealedObjectives = [],
  board,
  table,
  onInspectCard,
  reactionModes,
  onSetReactionMode,
}) => {
  const display = usePlayerIdentity();
  const publicState = usePublicState();
  const bluff = useBluffControls();
  const { isMuted, toggleMute } = useTurnSound();
  const [isMutedState, setIsMutedState] = useState(isMuted);
  const { muted: toastsMuted, toggleMute: toggleToastMute } = useToastMute();
  const { enabled: recapOn, toggle: toggleRecap } = useTurnRecapSetting();
  const { enabled: autoSubmit, toggle: toggleAutoSubmit } = useAutoSubmitSetting();

  // Sort players: current player first, then by turn order
  const sortedPlayers = React.useMemo(() => {
    if (players.length === 0) return [];

    // Find current player
    const currentPlayer = players.find(p => p.id === userSeat);

    // Create a map for quick turn order lookup
    const turnOrderMap = new Map(seatingOrder.map((id, index) => [id, index]));

    // Separate current player and others
    const otherPlayers = players.filter(p => p.id !== userSeat);

    // Sort others by turn order
    const sortedOthers = otherPlayers.sort((a, b) => {
      const orderA = turnOrderMap.get(a.id) ?? Infinity;
      const orderB = turnOrderMap.get(b.id) ?? Infinity;
      return orderA - orderB;
    });

    // Return current player first, then sorted others
    return currentPlayer ? [currentPlayer, ...sortedOthers] : sortedOthers;
  }, [players, userSeat, seatingOrder]);

  // Per printed card name and per game, held by the server: every copy of a card shares it.
  const getReactionMode = (cardName: string): ReactionModeSetting =>
    reactionModes?.[cardName] === "never" ? "never" : "always";

  return (
    <aside
      data-testid="player-sheet-panel"
      className="player-sheet-panel"
      style={{
        width: 330,
        height: "100%",
        borderLeft: "1px solid var(--color-border)",
        overflowY: "auto",
        padding: 12,
        display: "flex",
        flexDirection: "column",
        gap: 12,
      }}
    >
      <div
        style={{
          display: "flex",
          justifyContent: "space-between",
          alignItems: "center",
          paddingBottom: 6,
          borderBottom: "1px solid #334155",
        }}
      >
        <h2 style={{ fontSize: 16, fontWeight: "bold", margin: 0, color: "#94a3b8" }}>Players</h2>

        <div style={{ display: "flex", flexWrap: "wrap", justifyContent: "flex-end", gap: 6 }}>
        {userSeat && <SecondaryPrepModeToggle />}
        {/* Sound Settings Toggle */}
        <button
          type="button"
          data-testid="turn-sound-btn"
          aria-pressed={isMutedState}
          title={isMutedState ? "Turn the beep for your decisions back on" : "Turn off the beep for your decisions"}
          onClick={() => {
            const newMutedState = toggleMute();
            setIsMutedState(newMutedState);
          }}
          style={{
            background: "transparent",
            border: "1px solid #94a3b8",
            color: "#94a3b8",
            borderRadius: 4,
            padding: "4px 8px",
            cursor: "pointer",
            fontSize: 12,
            fontWeight: "500",
            transition: "all 0.2s ease",
            backgroundColor: isMutedState ? "rgba(148, 163, 184, 0.1)" : "transparent",
          }}
          onMouseEnter={(e) => {
            (e.target as HTMLButtonElement).style.backgroundColor = "rgba(148, 163, 184, 0.2)";
          }}
          onMouseLeave={(e) => {
            (e.target as HTMLButtonElement).style.backgroundColor = isMutedState
              ? "rgba(148, 163, 184, 0.1)"
              : "transparent";
          }}
        >
          {isMutedState ? "🔇 Sound off" : "🔊 Sound on"}
        </button>
        <button
          type="button"
          data-testid="toast-mute-btn"
          aria-pressed={toastsMuted}
          title={
            toastsMuted
              ? "Show notifications about other players' actions"
              : "Hide notifications about other players' actions"
          }
          onClick={toggleToastMute}
          style={{
            background: toastsMuted ? "rgba(148, 163, 184, 0.1)" : "transparent",
            border: "1px solid #94a3b8",
            color: "#94a3b8",
            borderRadius: 4,
            padding: "4px 8px",
            cursor: "pointer",
            fontSize: 12,
            fontWeight: "500",
          }}
        >
          {toastsMuted ? "🔕 Toasts off" : "🔔 Toasts"}
        </button>
        <button
          type="button"
          data-testid="turn-recap-btn"
          aria-pressed={recapOn}
          title={
            recapOn
              ? "Stop the recap after other players' turns"
              : "Show a short recap after each other player's turn"
          }
          onClick={toggleRecap}
          style={{
            background: recapOn ? "rgba(34, 197, 94, 0.14)" : "transparent",
            border: `1px solid ${recapOn ? "#22c55e" : "#94a3b8"}`,
            color: recapOn ? "#22c55e" : "#94a3b8",
            borderRadius: 4,
            padding: "4px 8px",
            cursor: "pointer",
            fontSize: 12,
            fontWeight: "500",
          }}
        >
          {recapOn ? "📝 Recap on" : "📝 Recap"}
        </button>
        {userSeat && (
          <button
            type="button"
            data-testid="auto-submit-btn"
            aria-pressed={autoSubmit}
            title={
              autoSubmit
                ? "Your only move (a lone strategic action, a lone system to activate) is taken for you. Click to be asked every time."
                : "You are asked even when there is only one move. Click to take lone moves automatically."
            }
            onClick={toggleAutoSubmit}
            style={{
              background: autoSubmit ? "transparent" : "rgba(148, 163, 184, 0.1)",
              border: "1px solid #94a3b8",
              color: "#94a3b8",
              borderRadius: 4,
              padding: "4px 8px",
              cursor: "pointer",
              fontSize: 12,
              fontWeight: "500",
            }}
          >
            {autoSubmit ? "⚡ Auto" : "⚡ Auto off"}
          </button>
        )}
        </div>
      </div>

      {sortedPlayers.map((player) => {
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
              padding: 12,
              borderLeft: `4px solid ${identity.color}`,
              display: "flex",
              flexDirection: "column",
              gap: 8,
            }}
          >
            {/* Header: current name + physical position + faction + VP */}
            <div style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}>
              <div>
                <strong style={{ fontSize: 15, color: "#f8fafc" }}>
                  {identity.position && <SeatBadge position={identity.position} />} {identity.label}{" "}
                  {isSelf && identity.label.trim().toLowerCase() !== "you" && "(You)"}
                </strong>
                <div
                  style={{ fontSize: 12, color: "#94a3b8", display: "flex", alignItems: "center", gap: 6 }}
                >
                  {player.faction}
                  <FactionInfoButton
                    faction={player.faction}
                    breakthroughHeld={publicState.breakthroughs[player.id] ?? null}
                    technologies={player.technologies}
                    leaders={player.leaders}
                  />
                </div>
              </div>
              <Tooltip
                content={<VPBreakdownTooltip breakdown={calculateVPBreakdown(player, board, table)} />}
                position="left"
              >
                <div
                  data-testid="player-vp"
                  style={{
                    background: "#fbbf24",
                    color: "#0f172a",
                    fontWeight: "bold",
                    fontSize: 14,
                    padding: "2px 8px",
                    borderRadius: 12,
                    cursor: "help",
                  }}
                  tabIndex={0}
                >
                  {player.victory_points} VP
                </div>
              </Tooltip>
            </div>

            {/* Economy & Tokens */}
            <div
              style={{
                display: "grid",
                gridTemplateColumns: "1fr 1fr",
                gap: 4,
                fontSize: 12,
                color: "#cbd5e1",
              }}
            >
              <div>
                <Tooltip content="Trade goods" as="span" position="bottom">
                  <span
                    data-testid="player-trade-goods"
                    aria-label={`Trade goods: ${player.trade_goods}`}
                    tabIndex={0}
                    style={{ cursor: "help" }}
                  >
                    TG: <strong>{player.trade_goods}</strong>
                  </span>
                </Tooltip>{" "}
                |{" "}
                <Tooltip content="Commodities" as="span" position="bottom">
                  <span
                    data-testid="player-commodities"
                    aria-label={`Commodities: ${player.commodities}`}
                    tabIndex={0}
                    style={{ cursor: "help" }}
                  >
                    Comm: <strong>{player.commodities}</strong>
                  </span>
                </Tooltip>
              </div>
              <div>
                <Tooltip
                  content={`Tactic ${player.tactic_tokens} · Fleet ${player.fleet_tokens} · Strategy ${player.strategic_tokens}`}
                  as="span"
                  position="bottom"
                >
                  <strong
                    data-testid="player-command-tokens"
                    aria-label={`Command tokens: tactic ${player.tactic_tokens}, fleet ${player.fleet_tokens}, strategy ${player.strategic_tokens}`}
                    tabIndex={0}
                    style={{ cursor: "help" }}
                  >
                    {player.tactic_tokens} ◆ {player.fleet_tokens} ◆ {player.strategic_tokens}
                  </strong>
                </Tooltip>
                {hasArmada(player.faction) && (
                  <Tooltip content={ARMADA_TEXT} as="span" position="bottom">
                    <span
                      className="token-panel__armada"
                      data-testid="player-armada"
                      aria-label={ARMADA_TEXT}
                      tabIndex={0}
                      style={{ cursor: "help", marginLeft: 6 }}
                    >
                      Fleet +{ARMADA_BONUS}
                    </span>
                  </Tooltip>
                )}
              </div>
            </div>

            {/* Other players' command tokens in this seat's fleet pool (Mahact's Edict). */}
            {(player.held_foreign_tokens ?? []).length > 0 && (
              <HeldForeignTokens owners={player.held_foreign_tokens ?? []} />
            )}

            {/* Additional Player Stats (Resources, Influence, Systems, Planets, Production) */}
            <div
              data-testid="player-stats-row"
              className="player-stats-row"
              style={{
                display: "flex",
                flexWrap: "wrap",
                gap: "3px 6px",
                alignItems: "center",
                padding: "3px 6px",
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
                  <PlanetValue
                    kind="resources"
                    value={stats.remainingResources}
                    total={stats.totalResources}
                    alwaysTotal
                    state={stats.remainingResources > 0 ? "ready" : "muted"}
                    label={`Resources: ${stats.remainingResources} ready / ${stats.totalResources} total`}
                  />
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
                  <PlanetValue
                    kind="influence"
                    value={stats.remainingInfluence}
                    total={stats.totalInfluence}
                    alwaysTotal
                    state={stats.remainingInfluence > 0 ? "ready" : "muted"}
                    label={`Influence: ${stats.remainingInfluence} ready / ${stats.totalInfluence} total`}
                  />
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
                <div style={{ color: "#94a3b8", marginBottom: 3, fontWeight: 500 }}>
                  Strategy Cards:
                </div>
                <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
                  {player.strategy_cards.map((scId) => {
                    const meta = getStrategyCardMeta(scId);
                    const isExhausted = player.exhausted_strategy_cards.includes(scId);
                    const tooltipText = `${meta.name} (Initiative ${meta.initiative})\n\nPrimary:\n${meta.primaryText}\n\nSecondary:\n${meta.secondaryText}`;
                    return (
                      <button
                        type="button"
                        key={scId}
                        onClick={() => onInspectCard?.({ kind: "strategy", id: scId })}
                        data-testid={`strategy-card-badge-${scId}`}
                        data-card-id={scId}
                        className={isExhausted ? "strategy-card--exhausted" : ""}
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

            <BreakthroughRow faction={player.faction} held={publicState.breakthroughs[player.id] ?? null} />

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
                  marginTop: 4,
                  borderTop: "1px dashed #475569",
                  paddingTop: 8,
                  display: "flex",
                  flexDirection: "column",
                  gap: 8,
                }}
              >
                <div style={{ fontSize: 12, fontWeight: "bold", color: "#38bdf8" }}>
                  Private Hand
                </div>

                {bluff && (
                  <BluffSelector
                    controls={bluff}
                    cardsCount={player.action_cards_count}
                    neverMode={Object.values(reactionModes ?? {}).some((mode) => mode === "never")}
                  />
                )}

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
                        gap: 4,
                      }}
                    >
                      {player.held_action_cards.map((cardId) => {
                        const meta = getActionCardMeta(cardId);
                        const tooltipText = `${meta.name} (${meta.phase ?? "Action"})\n\n${meta.description}`;
                        const mode = getReactionMode(meta.name);
                        const modeLabel = mode === "always" ? "✓" : "✕";
                        const modeColor = mode === "always" ? "#4ade80" : "#ef4444";
                        const modeTitle =
                          mode === "always"
                            ? `${meta.name} is offered when it fits a window (click to never offer it this game)`
                            : `${meta.name} is never offered this game (click to offer it again)`;

                        return (
                          <li
                            key={cardId}
                            data-private-card="true"
                            data-private-card-owner={
                              identity.position ? `position-${identity.position}` : "participant"
                            }
                            data-action-card-id={cardId}
                            data-testid={`action-card-item-${cardId}`}
                            className="card"
                            style={{
                              border: "1px solid #475569",
                              padding: "5px 8px",
                              cursor: "pointer",
                              position: "relative",
                              display: "flex",
                              gap: 6,
                              alignItems: "flex-start",
                            }}
                          >
                            <button
                              type="button"
                              className="detail-trigger"
                              onClick={() => onInspectCard?.({ kind: "action", id: cardId })}
                              title={tooltipText}
                              style={{ flex: 1, textAlign: "left" }}
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
                            {isSelf && onSetReactionMode && (
                              <button
                                type="button"
                                title={modeTitle}
                                aria-label={modeTitle}
                                aria-pressed={mode === "never"}
                                data-testid={`reaction-inspect-mode-${cardId}`}
                                data-reaction-mode={mode}
                                onClick={(e) => {
                                  e.stopPropagation();
                                  onSetReactionMode(
                                    meta.name,
                                    mode === "always" ? "never" : "always",
                                  );
                                }}
                                style={{
                                  background: "transparent",
                                  border: `1px solid ${modeColor}`,
                                  color: modeColor,
                                  fontSize: 14,
                                  fontWeight: "bold",
                                  padding: "2px 8px",
                                  borderRadius: 4,
                                  cursor: "pointer",
                                  minWidth: "32px",
                                  flexShrink: 0,
                                }}
                              >
                                {modeLabel}
                              </button>
                            )}
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
                        gap: 4,
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
                              padding: "5px 8px",
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
                                <SecretObjectiveProgress
                                  alias={objId}
                                  progress={secretProgressOf(player, objId)}
                                  board={board}
                                />
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
