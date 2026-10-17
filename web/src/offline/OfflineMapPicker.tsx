import React, { useEffect, useState } from "react";
import "../components/MapPicker.css";
import { MapCard } from "../components/MapCard.tsx";
import { MapPreviewBoard } from "../components/MapPreviewBoard.tsx";
import { useMapCatalog, useMapPreview, type MapDataSource } from "../hooks/useMapCatalog.ts";
import { mapCards, templateTitle } from "../presentation/mapPicker.ts";
import { seatStyle } from "../presentation/playerDisplay.ts";
import type { MapChoice, MapChoiceDto, MapTemplateSummary } from "../protocol/types.ts";
import type { OfflineMapSource } from "./types.ts";

/** The hooks' view of the engine's map source. Ids differ from the server's so cached pictures never mix. */
export function mapDataSourceOf(source: OfflineMapSource): MapDataSource {
  return {
    id: "offline",
    catalog: (playerCount) => source.catalog(playerCount),
    preview: (choice, playerCount, variant) => source.preview(choice, playerCount, variant),
  };
}

/** What the strip shows as picked: the choice, or the recommended layout while none was made. */
export function effectiveChoice(
  picked: MapChoice | null,
  templates: readonly MapTemplateSummary[],
): MapChoice | undefined {
  if (picked) return picked;
  const recommended = templates.find((t) => t.recommended);
  return recommended ? { kind: "template", alias: recommended.alias } : undefined;
}

/** `Standard (recommended)`, `Random map`: the form's one-line answer to "which map". */
export function mapLabel(
  picked: MapChoice | null,
  templates: readonly MapTemplateSummary[] | null,
): string {
  if (picked?.kind === "random") return "Random map";
  const alias =
    picked?.alias ?? templates?.find((t) => t.recommended)?.alias ?? null;
  if (!alias) return "Recommended layout";
  const found = templates?.find((t) => t.alias === alias);
  const title = found?.name ?? templateTitle(alias);
  return picked ? title : `${title} (recommended)`;
}

/**
 * "Choose your map" for offline games: the same strip of cards and the same large picture as the
 * lobby's picker, with the engine answering instead of the server. A pick only changes the form
 * (nothing is saved until the game starts). The seats and factions are drawn at the start, and so
 * is the random part of a layout, so the picture is an example of the choice.
 */
export const OfflineMapPicker: React.FC<{
  playerCount: number;
  value: MapChoice | null;
  onChange: (choice: MapChoice) => void;
  onClose: () => void;
}> = ({ playerCount, value, onChange, onClose }) => {
  const catalog = useMapCatalog(playerCount);
  const templates = catalog.state.status === "ready" ? catalog.state.value : [];
  const current = effectiveChoice(value, templates);
  const [variant, setVariant] = useState(0);
  const key = current ? (current.kind === "random" ? "random" : current.alias) : "";
  useEffect(() => setVariant(0), [key]);
  const preview = useMapPreview(
    current ? { kind: "card", choice: current, playerCount, variant } : null,
  );
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  const selected: MapChoiceDto | undefined = current
    ? current.kind === "random"
      ? { kind: "random", systems: 0, hyperlanes: false, recommended: false }
      : { kind: "template", alias: current.alias, systems: 0, hyperlanes: false, recommended: false }
    : undefined;
  const cards = mapCards(templates, selected);
  const summary = templates.find((t) => current?.kind === "template" && t.alias === current.alias);
  return (
    <div className="map-picker-backdrop" data-testid="offline-map-picker">
      <section className="map-picker panel" role="dialog" aria-modal="true" aria-label="Choose your map">
        <header className="map-picker__header">
          <h2>Choose your map</h2>
          <span className="text-muted">{playerCount} players</span>
        </header>
        <div className="map-picker__choices" data-testid="map-picker-choices">
          {catalog.state.status === "loading" && (
            <div className="map-picker__strip" data-testid="map-picker-loading" role="status" aria-label="Loading maps">
              {[0, 1, 2, 3].map((n) => (
                <span className="map-card map-card--skeleton" key={n} aria-hidden="true" />
              ))}
            </div>
          )}
          {catalog.state.status === "error" && (
            <div className="map-picker__message" role="alert" data-testid="map-picker-error">
              <p>Could not load the maps ({catalog.state.message}).</p>
              <button type="button" className="button button--outline" onClick={catalog.retry}>
                Retry
              </button>
            </div>
          )}
          {catalog.state.status === "ready" && (
            <>
              {templates.length === 0 && (
                <p className="map-picker__message text-muted" data-testid="map-picker-empty">
                  No ready-made layout fits {playerCount} players. You can still play on a random map.
                </p>
              )}
              <div className="map-picker__strip">
                {cards.map((card) => (
                  <MapCard
                    key={card.key}
                    card={card}
                    playerCount={playerCount}
                    disabled={false}
                    saving={false}
                    onChoose={() => onChange(card.choice)}
                  />
                ))}
              </div>
            </>
          )}
        </div>
        <div className="map-picker__preview" data-testid="map-picker-preview">
          {preview.state.status === "loading" && current && (
            <p className="text-muted map-picker__message" role="status">
              Loading the map…
            </p>
          )}
          {preview.state.status === "error" && (
            <div className="map-picker__message" role="alert" data-testid="map-preview-error">
              <p>Could not load the preview ({preview.state.message}).</p>
              <button type="button" className="button button--outline" onClick={preview.retry}>
                Retry
              </button>
            </div>
          )}
          {preview.state.status === "ready" && current && (
            <>
              <MapPreviewBoard preview={preview.state.value} />
              <ul className="map-picker__seats" aria-label="Seats">
                {preview.state.value.seats.map((s) => (
                  <li key={s.seat} style={{ borderColor: seatStyle(s.seat).color }}>
                    Seat {s.seat}
                  </li>
                ))}
              </ul>
            </>
          )}
        </div>
        <footer className="map-picker__footer">
          <p className="map-picker__summary" data-testid="map-picker-summary">
            {mapLabel(value, templates)}
            {summary ? ` · ${summary.systems} systems · ${summary.hyperlanes ? "with hyperlanes" : "no hyperlanes"}` : ""}
            {". The picture is an example: seats, factions and the open tiles are drawn when the game starts."}
          </p>
          <div className="map-picker__actions">
            <button
              type="button"
              className="button button--outline"
              data-testid="map-example"
              disabled={!current}
              onClick={() => setVariant((n) => n + 1)}
            >
              Another example
            </button>
            <button type="button" className="button button--primary" data-testid="map-picker-done" onClick={onClose}>
              Done
            </button>
          </div>
        </footer>
      </section>
    </div>
  );
};
