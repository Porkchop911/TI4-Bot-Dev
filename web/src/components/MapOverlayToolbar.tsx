import React from "react";
import { InfluenceIcon, ResourceIcon } from "./PlanetValueIcons.tsx";
import { Tooltip } from "../primitives/index.ts";
import { MapOverlayMode } from "../presentation/mapOverlays.ts";

export interface MapOverlayToolbarProps {
  activeMode: MapOverlayMode;
  onSelectMode: (mode: MapOverlayMode) => void;
  /** When set, overlay switching is unavailable and the reason is shown as the tooltip. */
  disabledReason?: string;
}

interface OverlayOption {
  mode: MapOverlayMode;
  label: string;
  icon: string;
  description: string;
  testId: string;
}

const OVERLAY_OPTIONS: OverlayOption[] = [
  {
    mode: "economy",
    label: "Resources and influence",
    icon: "💰",
    description: "Resources & Influence (Ready vs Exhausted)",
    testId: "overlay-btn-economy",
  },
  {
    mode: "space_combat",
    label: "Space Combat",
    icon: "🚀",
    description: "Space Combat: Units & Avg Hits / Round",
    testId: "overlay-btn-space_combat",
  },
  {
    mode: "ground_combat",
    label: "Ground Combat",
    icon: "🪖",
    description: "Ground Combat: Defenders, Avg Hits & Planetary Shields",
    testId: "overlay-btn-ground_combat",
  },
  {
    mode: "tech_benefits",
    label: "Tech Skips",
    icon: "🔬",
    description: "Tech Specialties & Planet Skips",
    testId: "overlay-btn-tech_benefits",
  },
];

export const MapOverlayToolbar: React.FC<MapOverlayToolbarProps> = ({
  activeMode,
  onSelectMode,
  disabledReason,
}) => {
  const disabled = Boolean(disabledReason);
  return (
    <div
      className="map-overlay-toolbar"
      data-testid="map-overlay-toolbar"
      role="toolbar"
      aria-label="Map Overlays"
      title={disabledReason}
      style={{
        display: "flex",
        alignItems: "center",
        gap: 4,
        background: "rgba(15, 23, 42, 0.88)",
        backdropFilter: "blur(6px)",
        padding: "3px 6px",
        borderRadius: 8,
        border: "1px solid rgba(71, 85, 105, 0.4)",
        boxShadow: "0 4px 12px rgba(0, 0, 0, 0.3)",
      }}
    >
      <Tooltip content="Standard Map View (Clear Overlay)" position="bottom">
        <button
          type="button"
          data-testid="overlay-btn-none"
          onClick={() => onSelectMode("none")}
          disabled={disabled}
          aria-pressed={activeMode === "none"}
          className={`button ${activeMode === "none" ? "button--primary" : "button--secondary"}`}
          style={{
            fontSize: 12,
            padding: "4px 8px",
            height: 28,
            display: "flex",
            alignItems: "center",
            gap: 4,
            borderRadius: 6,
          }}
        >
          <span>Standard</span>
        </button>
      </Tooltip>

      <div
        style={{
          width: 1,
          height: 18,
          background: "rgba(100, 116, 139, 0.4)",
          margin: "0 2px",
        }}
        aria-hidden="true"
      />

      {OVERLAY_OPTIONS.map((opt) => {
        const isActive = activeMode === opt.mode;
        return (
          <Tooltip key={opt.mode} content={opt.description} position="bottom">
            <button
              type="button"
              data-testid={opt.testId}
              onClick={() => onSelectMode(isActive ? "none" : opt.mode)}
              disabled={disabled}
              aria-pressed={isActive}
              aria-label={opt.mode === "economy" ? "Resources and influence overlay" : undefined}
              className={`button ${isActive ? "button--primary" : "button--secondary"}`}
              style={{
                fontSize: 12,
                padding: "4px 8px",
                height: 28,
                display: "flex",
                alignItems: "center",
                gap: 5,
                borderRadius: 6,
                fontWeight: isActive ? 600 : 400,
              }}
            >
              {opt.mode === "economy" ? (
                <>
                  <ResourceIcon />
                  <InfluenceIcon />
                </>
              ) : (
                <>
                  <span aria-hidden="true">{opt.icon}</span>
                  <span>{opt.label}</span>
                </>
              )}
            </button>
          </Tooltip>
        );
      })}
    </div>
  );
};
