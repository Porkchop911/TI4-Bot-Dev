import React from "react";
import { useMapPreview } from "../hooks/useMapCatalog.ts";
import type { MapCardModel } from "../presentation/mapPicker.ts";
import { MapThumbnail } from "./MapThumbnail.tsx";

/** One choice in the picker's strip: a small picture, its name, and honest labels. */
export const MapCard: React.FC<{
  card: MapCardModel;
  playerCount: number;
  disabled: boolean;
  saving: boolean;
  onChoose: () => void;
}> = ({ card, playerCount, disabled, saving, onChoose }) => {
  const { state } = useMapPreview({
    kind: "card",
    choice: card.choice,
    playerCount,
  });
  return (
    <button
      type="button"
      className={`map-card${card.selected ? " map-card--selected" : ""}`}
      data-testid={`map-card-${card.key}`}
      aria-pressed={card.selected}
      disabled={disabled}
      onClick={onChoose}
    >
      <span className="map-card__picture">
        {state.status === "ready" ? (
          <MapThumbnail
            tiles={state.value.tiles}
            seats={state.value.seats}
            label={`${card.title} layout`}
          />
        ) : (
          <span
            className={`map-card__placeholder${state.status === "error" ? " map-card__placeholder--error" : ""}`}
            aria-hidden="true"
          >
            {state.status === "error" ? "!" : ""}
          </span>
        )}
        {saving && (
          <span className="map-card__spinner" role="status" aria-label="Saving">
            …
          </span>
        )}
      </span>
      <strong className="map-card__title">{card.title}</strong>
      <span className="map-card__detail">{card.detail}</span>
      {card.recommended && <span className="map-card__badge">Recommended</span>}
      {card.badges.map((badge) => (
        <span className="map-card__badge map-card__badge--note" key={badge}>
          {badge}
        </span>
      ))}
    </button>
  );
};
