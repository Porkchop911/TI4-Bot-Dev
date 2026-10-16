import React from "react";
import {
  SelectedSystemDetails,
} from "../presentation/boardPresentation.ts";
import { buildOwnerBlocks, type GroupedUnit } from "../presentation/unitsByOwner.ts";
import { DetailPanel } from "./DetailPanel.tsx";
import { PlanetValuePair } from "./PlanetValueIcons.tsx";
import { PlanetStructures } from "./PlanetStructures.tsx";
import { splitStructures } from "../presentation/planetStructures.ts";
import { UnitStatsTip } from "./UnitInfo.tsx";
import { describeAnomaly } from "../presentation/anomalyRules.ts";
import { findPlanetMeta } from "../protocol/contentCatalog.ts";
import { attachmentLabels, explorationsInSystem } from "../presentation/explorationMarks.ts";
import { usePublicState } from "../presentation/PublicStateContext.tsx";
import { SeatBadge, usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import {
  UnitIcon,
  getUnitDisplayName,
} from "./UnitIcon.tsx";

export type InspectorTab = "details" | "reach";

export interface SystemInspectorProps {
  system: SelectedSystemDetails | null;
  onClose: () => void;
  onSelectAction?: (optionId: string) => void;
  /** The viewer's block of units comes first; the others follow in this table order. */
  viewerSeat?: string | null;
  seatingOrder?: readonly string[];
  /** Present when the "Who can go here" tab is offered; absent, the panel is the plain overview. */
  reach?: {
    tab: InspectorTab;
    onTab: (tab: InspectorTab) => void;
    content: React.ReactNode;
  };
}

export const SystemInspector: React.FC<SystemInspectorProps> = ({
  system,
  onClose,
  reach,
  viewerSeat = null,
  seatingOrder = [],
}) => {
  const display = usePlayerIdentity();
  const { explorations, frontierSystems } = usePublicState();
  if (!system) return null;
  const explored = explorationsInSystem(
    explorations,
    system.systemId,
    system.planets.map((p) => p.id),
  );
  const hasFrontierToken = frontierSystems.has(system.systemId);
  const blocks = buildOwnerBlocks(system, viewerSeat, seatingOrder);
  const planetLabel = (id: string) => system.planets.find((p) => p.id === id)?.label ?? id;

  return (
    <DetailPanel
      title={`System ${system.label} #${system.systemId}`}
      onClose={onClose}
      testId="system-inspector"
      closeTestId="close-inspector-button"
    >
      {/* Header */}
      <div style={{ display: "flex", justifyContent: "space-between", alignItems: "flex-start" }}>
        <div>
          <div
            style={{
              fontSize: 11,
              fontWeight: "bold",
              color: "#38bdf8",
              textTransform: "uppercase",
            }}
          >
            System Details
          </div>
          <h3
            data-testid="inspector-system-title"
            style={{ margin: "2px 0 0 0", fontSize: 16, fontWeight: 700, color: "#f8fafc" }}
          >
            {system.label}{" "}
            <span style={{ color: "#94a3b8", fontSize: 13 }}>#{system.systemId}</span>
          </h3>
          {system.isActiveSystem && (
            <span
              style={{
                display: "inline-block",
                marginTop: 4,
                padding: "2px 6px",
                borderRadius: 4,
                fontSize: 10,
                fontWeight: 700,
                background: "rgba(56, 189, 248, 0.2)",
                color: "#38bdf8",
                border: "1px solid #38bdf8",
              }}
            >
              ACTIVE SYSTEM
            </span>
          )}
        </div>
      </div>

      {reach && (
        <div role="tablist" aria-label="System overview" style={{ display: "flex", gap: 6 }}>
          {(
            [
              ["details", "Details"],
              ["reach", "Who can go here"],
            ] as const
          ).map(([id, label]) => (
            <button
              key={id}
              type="button"
              role="tab"
              aria-selected={reach.tab === id}
              data-testid={`inspector-tab-${id}`}
              onClick={() => reach.onTab(id)}
              style={{
                flex: 1,
                minHeight: 44,
                padding: "4px 8px",
                borderRadius: 6,
                font: "inherit",
                fontSize: 12,
                fontWeight: 700,
                cursor: "pointer",
                color: reach.tab === id ? "#0b1220" : "#cbd5e1",
                background: reach.tab === id ? "#38bdf8" : "#1e293b",
                border: `1px solid ${reach.tab === id ? "#38bdf8" : "#334155"}`,
              }}
            >
              {label}
            </button>
          ))}
        </div>
      )}

      {reach?.tab === "reach" ? (
        reach.content
      ) : (
      <>
      {/* Anomalies & Wormholes */}
      {(system.anomalies.length > 0 || system.wormholes.length > 0) && (
        <div style={{ display: "flex", flexWrap: "wrap", gap: 6 }}>
          {system.anomalies.map((a, i) => (
            <span
              key={i}
              data-testid="inspector-anomaly"
              style={{
                padding: "2px 6px",
                borderRadius: 4,
                fontSize: 11,
                fontWeight: 600,
                background: "#451a03",
                color: "#fef08a",
                border: "1px solid #d97706",
              }}
            >
              {a.toUpperCase()}
            </span>
          ))}
          {system.wormholes.map((wh, i) => (
            <span
              key={i}
              data-testid="inspector-wormhole"
              style={{
                padding: "2px 6px",
                borderRadius: 4,
                fontSize: 11,
                fontWeight: 600,
                background: "#2e1065",
                color: "#c084fc",
                border: "1px solid #9333ea",
              }}
            >
              WORMHOLE: {wh.toUpperCase()}
            </span>
          ))}
        </div>
      )}

      {system.anomalies.map((a) => {
        const rule = describeAnomaly(a);
        return rule ? (
          <p
            key={a}
            data-testid="inspector-anomaly-rule"
            style={{ margin: 0, fontSize: 12, color: "#fde68a", lineHeight: 1.4 }}
          >
            <strong>{rule.name}:</strong> {rule.text}
          </p>
        ) : null;
      })}

      {/* Planets */}
      <div>
        <h4
          style={{
            margin: "0 0 6px 0",
            fontSize: 12,
            fontWeight: 700,
            color: "#94a3b8",
            textTransform: "uppercase",
          }}
        >
          Planets ({system.planets.length})
        </h4>
        {system.planets.length === 0 ? (
          <div style={{ color: "#64748b", fontSize: 12 }}>No planets in this system.</div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            {system.planets.map((p) => (
              <div
                key={p.id}
                data-testid={`inspector-planet-${p.id}`}
                style={{
                  padding: "8px 10px",
                  borderRadius: 6,
                  background: "rgba(30, 41, 59, 0.7)",
                  border: `1px solid ${p.isCandidateTarget ? "#38bdf8" : "#334155"}`,
                }}
              >
                <div
                  style={{ display: "flex", justifyContent: "space-between", alignItems: "center" }}
                >
                  <span style={{ fontWeight: 600, color: "#f8fafc" }}>
                    {p.label}{" "}
                    {p.legendary && (
                      <span
                        style={{ fontSize: 10, color: "#facc15", fontWeight: 700 }}
                        title="Legendary Planet"
                      >
                        ★
                      </span>
                    )}
                  </span>
                  <span style={{ fontSize: 12, color: "#facc15", fontWeight: 600 }}>
                    <PlanetValuePair resources={p.resources} influence={p.influence} size="tooltip" />
                  </span>
                </div>

                <div
                  style={{
                    display: "flex",
                    justifyContent: "space-between",
                    alignItems: "center",
                    marginTop: 4,
                    fontSize: 11,
                  }}
                >
                  <span>
                    Control:{" "}
                    {p.controlledBy ? (
                      <span
                        style={{
                          color: "#f8fafc",
                          borderLeft: `3px solid ${p.controllerColor}`,
                          paddingLeft: 3,
                          fontWeight: 600,
                        }}
                      >
                        {display(p.controlledBy).position && (
                          <SeatBadge position={display(p.controlledBy).position!} />
                        )}{" "}
                        {display(p.controlledBy).label} {p.exhausted ? "(Exhausted)" : "(Ready)"}
                      </span>
                    ) : (
                      <span style={{ color: "#64748b" }}>Uncontrolled</span>
                    )}
                  </span>
                  {p.traits.length > 0 && (
                    <span style={{ color: "#94a3b8" }}>{p.traits.join(", ")}</span>
                  )}
                </div>

                {p.attachments.length > 0 && (
                  <div style={{ marginTop: 4, fontSize: 11, color: "#a78bfa" }}>
                    Attachments: {attachmentLabels(p.attachments).join(", ")}
                  </div>
                )}

                {p.legendary && <LegendaryAbility planetId={p.id} />}

                <PlanetStructures planetId={p.id} units={system.planetUnits[p.id] ?? []} />
              </div>
            ))}
          </div>
        )}
      </div>

      {(explored.length > 0 || hasFrontierToken) && (
        <div data-testid="inspector-exploration">
          <h4 style={{ margin: "0 0 6px 0", fontSize: 11, color: "#94a3b8", textTransform: "uppercase" }}>
            Exploration
          </h4>
          {hasFrontierToken && (
            <div data-testid="inspector-frontier-token" style={{ fontSize: 12, color: "#fbbf24", marginBottom: 4 }}>
              Frontier token: unexplored. A player with a ship here can explore it.
            </div>
          )}
          <ul style={{ margin: 0, padding: 0, listStyle: "none", display: "grid", gap: 4 }}>
            {explored.map((e, index) => (
              <li
                key={`${e.card}-${index}`}
                data-testid="inspector-exploration-entry"
                data-kind={e.kind}
                style={{ fontSize: 12, color: "#cbd5e1", borderLeft: "3px solid #2dd4bf", paddingLeft: 6 }}
              >
                <strong style={{ color: "#f8fafc" }}>{e.card}</strong> at {e.where}
                <div>
                  {display(e.player).label}
                  {e.round ? `, round ${e.round}` : ""}: {e.result}
                </div>
              </li>
            ))}
          </ul>
        </div>
      )}

      {/* Units: one block per owner, never mixed */}
      <div>
        <h4
          style={{
            margin: "0 0 6px 0",
            fontSize: 12,
            fontWeight: 700,
            color: "#94a3b8",
            textTransform: "uppercase",
          }}
        >
          Units (
          {system.spaceUnits.length +
            Object.values(system.planetUnits).flatMap((l) => splitStructures(l).others).length}
          )
        </h4>
        {blocks.length === 0 ? (
          <div style={{ color: "#64748b", fontSize: 11, marginLeft: 6 }}>No units</div>
        ) : (
          <div style={{ display: "flex", flexDirection: "column", gap: 8 }}>
            {blocks.map((block) => {
              const pInfo = display(block.owner);
              const playerColor = block.ownerColor || pInfo.color;
              return (
                <div
                  key={block.owner}
                  data-testid={`inspector-owner-${block.owner}`}
                  style={{
                    background: "rgba(15, 23, 42, 0.6)",
                    border: "1px solid rgba(51, 65, 85, 0.7)",
                    borderLeft: `3px solid ${playerColor}`,
                    borderRadius: 4,
                    padding: "6px 8px",
                    display: "flex",
                    flexDirection: "column",
                    gap: 6,
                  }}
                >
                  <div
                    style={{
                      display: "flex",
                      alignItems: "center",
                      gap: 6,
                      fontSize: 12,
                      fontWeight: 700,
                      color: "#f8fafc",
                    }}
                  >
                    {pInfo.position && <SeatBadge position={pInfo.position} />}
                    <span>{pInfo.label}</span>
                    {block.owner === viewerSeat && (
                      <span style={{ fontSize: 10, color: "#38bdf8" }}>(you)</span>
                    )}
                  </div>
                  {block.space.length > 0 && (
                    <div data-testid={`inspector-player-space-${block.owner}`}>
                      <div style={{ fontSize: 10, fontWeight: 600, color: "#94a3b8", marginBottom: 3 }}>
                        In space
                      </div>
                      <UnitChips
                        units={block.space}
                        owner={block.owner}
                        color={playerColor}
                        testId="inspector-space-unit"
                      />
                    </div>
                  )}
                  {block.planets.map((pl) => (
                    <div
                      key={pl.planetId}
                      data-testid={`inspector-player-ground-${pl.planetId}-${block.owner}`}
                    >
                      <div style={{ fontSize: 10, fontWeight: 600, color: "#94a3b8", marginBottom: 3 }}>
                        On {planetLabel(pl.planetId)}
                      </div>
                      {pl.ground.length > 0 && (
                        <UnitChips
                          units={pl.ground}
                          owner={block.owner}
                          color={playerColor}
                          testId="inspector-ground-unit"
                        />
                      )}
                      {pl.structures.length > 0 && (
                        <div
                          data-testid={`inspector-owner-structures-${pl.planetId}-${block.owner}`}
                          style={{ fontSize: 11, color: "#cbd5e1", marginTop: 3 }}
                        >
                          {pl.structures
                            .map(
                              (g) =>
                                `${g.count > 1 ? `${g.count} × ` : ""}${getUnitDisplayName(g.unitType, g.count)}`,
                            )
                            .join(", ")}
                        </div>
                      )}
                    </div>
                  ))}
                </div>
              );
            })}
          </div>
        )}
      </div>

      {/* Command Tokens */}
      {system.commandTokens.length > 0 && (
        <div>
          <h4
            style={{
              margin: "0 0 4px 0",
              fontSize: 12,
              fontWeight: 700,
              color: "#94a3b8",
              textTransform: "uppercase",
            }}
          >
            Command Tokens
          </h4>
          <div style={{ display: "flex", gap: 6, flexWrap: "wrap" }}>
            {system.commandTokens.map((ct, i) => (
              <span
                key={i}
                data-testid="inspector-command-token"
                style={{
                  display: "inline-flex",
                  alignItems: "center",
                  gap: 4,
                  fontSize: 11,
                  padding: "2px 6px",
                  borderRadius: 4,
                  background: "#1e293b",
                  color: "#e2e8f0",
                }}
              >
                <span
                  style={{
                    width: 8,
                    height: 8,
                    borderRadius: "50%",
                    background: ct.color,
                    outline: "1px solid #f8fafc",
                  }}
                />
                {display(ct.owner).position && <SeatBadge position={display(ct.owner).position!} />}{" "}
                {display(ct.owner).label}
              </span>
            ))}
          </div>
        </div>
      )}
      </>
      )}
    </DetailPanel>
  );
};

const UnitChips: React.FC<{
  units: readonly GroupedUnit[];
  owner: string;
  color: string;
  testId: string;
}> = ({ units, owner, color, testId }) => (
  <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
    {units.map((u) => (
      <span
        key={u.baseType}
        data-testid={testId}
        style={{
          display: "inline-flex",
          alignItems: "center",
          gap: 5,
          padding: "2px 6px",
          borderRadius: 4,
          fontSize: 11,
          background: "#1e293b",
          color: u.damagedCount > 0 ? "#fca5a5" : "#e2e8f0",
        }}
      >
        <UnitStatsTip unit={u.baseType} seat={owner}>
          <UnitIcon type={u.baseType} size={14} color={color} aria-hidden="true" />
          <span>
            {u.totalCount} × {getUnitDisplayName(u.baseType, u.totalCount)}
          </span>
        </UnitStatsTip>
        {u.damagedCount > 0 && (
          <span style={{ fontSize: 10, fontWeight: 600, color: "#f87171" }}>
            {" "}
            ({u.damagedCount} damaged)
          </span>
        )}
      </span>
    ))}
  </div>
);

/** The ability card a legendary planet carries, so the overview needs no trip to the card list. */
const LegendaryAbility: React.FC<{ planetId: string }> = ({ planetId }) => {
  const meta = findPlanetMeta(planetId);
  const text = meta?.legendaryAbilityText?.trim();
  if (!meta?.legendaryAbilityName && !text) return null;
  return (
    <div
      data-testid={`inspector-legendary-${planetId}`}
      style={{ marginTop: 4, fontSize: 11, color: "#fde68a", lineHeight: 1.4 }}
    >
      <strong>{meta?.legendaryAbilityName ?? "Legendary ability"}</strong>
      {text ? `: ${text}` : ""}
    </div>
  );
};
