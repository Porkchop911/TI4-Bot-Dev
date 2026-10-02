import React from "react";
import {
  SelectedSystemDetails,
  PlacedUnitPresentation,
} from "../presentation/boardPresentation.ts";
import { DetailPanel } from "./DetailPanel.tsx";
import { SeatBadge, usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import {
  UnitIcon,
  getUnitBaseType,
  getUnitDisplayName,
  UNIT_PRIORITY,
  UnitBaseType,
} from "./UnitIcon.tsx";

export interface SystemInspectorProps {
  system: SelectedSystemDetails | null;
  onClose: () => void;
  onSelectAction?: (optionId: string) => void;
}

export const SystemInspector: React.FC<SystemInspectorProps> = ({ system, onClose }) => {
  const display = usePlayerIdentity();
  if (!system) return null;

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
                    {p.resources} Res / {p.influence} Inf
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
                    Attachments: {p.attachments.join(", ")}
                  </div>
                )}
              </div>
            ))}
          </div>
        )}
      </div>

      {/* Units */}
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
          Units ({system.spaceUnits.length + Object.values(system.planetUnits).flat().length})
        </h4>

        {/* Space Units */}
        <div style={{ marginBottom: 8 }}>
          <div style={{ fontSize: 11, fontWeight: 600, color: "#cbd5e1", marginBottom: 4 }}>
            Space Roster:
          </div>
          {system.spaceUnits.length === 0 ? (
            <div style={{ color: "#64748b", fontSize: 11, marginLeft: 6 }}>No space units</div>
          ) : (
            <div style={{ display: "flex", flexDirection: "column", gap: 6, marginLeft: 6 }}>
              {groupUnitsByPlayer(system.spaceUnits).map((pg) => {
                const pInfo = display(pg.owner);
                const playerColor = pg.ownerColor || pInfo.color;
                return (
                  <div
                    key={pg.owner}
                    data-testid={`inspector-player-space-${pg.owner}`}
                    style={{
                      background: "rgba(15, 23, 42, 0.6)",
                      border: "1px solid rgba(51, 65, 85, 0.7)",
                      borderLeft: `3px solid ${playerColor}`,
                      borderRadius: 4,
                      padding: "6px 8px",
                    }}
                  >
                    <div
                      style={{
                        display: "flex",
                        alignItems: "center",
                        gap: 6,
                        marginBottom: 4,
                        fontSize: 11,
                        fontWeight: 600,
                        color: "#f8fafc",
                      }}
                    >
                      {pInfo.position && <SeatBadge position={pInfo.position} />}
                      <span>{pInfo.label}</span>
                    </div>
                    <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
                      {pg.units.map((u) => (
                        <span
                          key={u.baseType}
                          data-testid="inspector-space-unit"
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
                          <UnitIcon
                            type={u.baseType}
                            size={14}
                            color={playerColor}
                            aria-hidden="true"
                          />
                          <span>
                            {u.totalCount} × {getUnitDisplayName(u.baseType, u.totalCount)}
                          </span>
                          {u.damagedCount > 0 && (
                            <span
                              style={{
                                fontSize: 10,
                                fontWeight: 600,
                                color: "#f87171",
                              }}
                            >
                              {" "}
                              ({u.damagedCount} damaged)
                            </span>
                          )}
                        </span>
                      ))}
                    </div>
                  </div>
                );
              })}
            </div>
          )}
        </div>

        {/* Ground Units */}
        {Object.entries(system.planetUnits).map(([pId, pUnits]) => {
          const planet = system.planets.find((p) => p.id === pId);
          const planetLabel = planet?.label || pId;
          const playerGroups = groupUnitsByPlayer(pUnits);

          if (pUnits.length === 0) return null;

          return (
            <div key={pId} style={{ marginTop: 6 }}>
              <div style={{ fontSize: 11, fontWeight: 600, color: "#cbd5e1", marginBottom: 4 }}>
                On {planetLabel}:
              </div>
              <div style={{ display: "flex", flexDirection: "column", gap: 6, marginLeft: 6 }}>
                {playerGroups.map((pg) => {
                  const pInfo = display(pg.owner);
                  const playerColor = pg.ownerColor || pInfo.color;
                  return (
                    <div
                      key={pg.owner}
                      data-testid={`inspector-player-ground-${pId}-${pg.owner}`}
                      style={{
                        background: "rgba(15, 23, 42, 0.6)",
                        border: "1px solid rgba(51, 65, 85, 0.7)",
                        borderLeft: `3px solid ${playerColor}`,
                        borderRadius: 4,
                        padding: "6px 8px",
                      }}
                    >
                      <div
                        style={{
                          display: "flex",
                          alignItems: "center",
                          gap: 6,
                          marginBottom: 4,
                          fontSize: 11,
                          fontWeight: 600,
                          color: "#f8fafc",
                        }}
                      >
                        {pInfo.position && <SeatBadge position={pInfo.position} />}
                        <span>{pInfo.label}</span>
                      </div>
                      <div style={{ display: "flex", flexWrap: "wrap", gap: 4 }}>
                        {pg.units.map((u) => (
                          <span
                            key={u.baseType}
                            data-testid="inspector-ground-unit"
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
                            <UnitIcon
                              type={u.baseType}
                              size={14}
                              color={playerColor}
                              aria-hidden="true"
                            />
                            <span>
                              {u.totalCount} × {getUnitDisplayName(u.baseType, u.totalCount)}
                            </span>
                            {u.damagedCount > 0 && (
                              <span
                                style={{
                                  fontSize: 10,
                                  fontWeight: 600,
                                  color: "#f87171",
                                }}
                              >
                                {" "}
                                ({u.damagedCount} damaged)
                              </span>
                            )}
                          </span>
                        ))}
                      </div>
                    </div>
                  );
                })}
              </div>
            </div>
          );
        })}
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
    </DetailPanel>
  );
};

interface GroupedUnit {
  baseType: UnitBaseType;
  totalCount: number;
  damagedCount: number;
}

interface PlayerUnitGroup {
  owner: string;
  ownerColor: string;
  units: GroupedUnit[];
}

function groupUnitsByPlayer(units: PlacedUnitPresentation[]): PlayerUnitGroup[] {
  const playerMap = new Map<
    string,
    { ownerColor: string; typeMap: Map<UnitBaseType, { total: number; damaged: number }> }
  >();

  for (const u of units) {
    let pEntry = playerMap.get(u.owner);
    if (!pEntry) {
      pEntry = { ownerColor: u.ownerColor, typeMap: new Map() };
      playerMap.set(u.owner, pEntry);
    }
    const baseType = getUnitBaseType(u.unitType);
    const existing = pEntry.typeMap.get(baseType) ?? { total: 0, damaged: 0 };
    existing.total += 1;
    if (u.damaged) {
      existing.damaged += 1;
    }
    pEntry.typeMap.set(baseType, existing);
  }

  const result: PlayerUnitGroup[] = [];
  for (const [owner, { ownerColor, typeMap }] of playerMap.entries()) {
    const groupedUnits: GroupedUnit[] = Array.from(typeMap.entries())
      .map(([baseType, counts]) => ({
        baseType,
        totalCount: counts.total,
        damagedCount: counts.damaged,
      }))
      .sort((a, b) => (UNIT_PRIORITY[a.baseType] ?? 99) - (UNIT_PRIORITY[b.baseType] ?? 99));

    result.push({
      owner,
      ownerColor,
      units: groupedUnits,
    });
  }

  return result;
}
