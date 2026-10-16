import React, { useEffect, useState } from "react";
import "./MapPicker.css";
import { useMapCatalog, useMapPreview } from "../hooks/useMapCatalog.ts";
import { choiceOf, mapCards, mapSummaryLine } from "../presentation/mapPicker.ts";
import type { LobbyDto, MapChoice } from "../protocol/types.ts";
import { seatStyle } from "../presentation/playerDisplay.ts";
import { MapCard } from "./MapCard.tsx";
import { MapPreviewBoard } from "./MapPreviewBoard.tsx";

/** Opening-state presets the server accepts (`crates/ti4-server/src/preset.rs`), dev builds only. */
const DEV_START_PRESETS = ["combat"];

interface MapPickerProps {
  lobby: LobbyDto;
  /** Host before Start: the strip of choices and Re-roll are shown. Everyone else gets the picture. */
  editable: boolean;
  /** A choice is being saved. */
  saving: boolean;
  /** `startPreset` is the dev opening-state preset ("" clears it); undefined leaves it alone. */
  onChoose: (choice: MapChoice, startPreset?: string) => void;
  onClose: () => void;
}

/**
 * "Choose your map": a full-screen sheet on phones, a wide dialog on desktop. The host picks
 * (every pick is saved at once and can be changed until Start); everyone else sees the same
 * large picture read-only. Previews come from the server.
 */
export const MapPicker: React.FC<MapPickerProps> = ({
  lobby,
  editable,
  saving,
  onChoose,
  onClose,
}) => {
  const map = lobby.map;
  const [preset, setPreset] = useState("");
  // Only chosen presets are sent, so an untouched picker never changes the lobby's preset.
  const [presetTouched, setPresetTouched] = useState(false);
  const choose = (choice: MapChoice) => onChoose(choice, presetTouched ? preset : undefined);
  const playerCount = lobby.slots.length;
  const catalog = useMapCatalog(editable ? playerCount : 0);
  const preview = useMapPreview(
    map
      ? {
          kind: "lobby",
          gameId: lobby.game_id,
          revision: lobby.map_revision ?? 0,
        }
      : null,
  );
  useEffect(() => {
    const onKey = (event: KeyboardEvent) => event.key === "Escape" && onClose();
    window.addEventListener("keydown", onKey);
    return () => window.removeEventListener("keydown", onKey);
  }, [onClose]);
  if (!map) return null;
  const current = choiceOf(map);
  const templates = catalog.state.status === "ready" ? catalog.state.value : [];
  const cards = mapCards(templates, map);
  return (
    <div className="map-picker-backdrop" data-testid="map-picker">
      <section
        className="map-picker panel"
        role="dialog"
        aria-modal="true"
        aria-label={editable ? "Choose your map" : "Map"}
      >
        <header className="map-picker__header">
          <h2>{editable ? "Choose your map" : "The table's map"}</h2>
          <span className="text-muted">{playerCount} players</span>
          {!editable && (
            <button
              type="button"
              className="button button--outline"
              data-testid="map-picker-close"
              onClick={onClose}
            >
              Close
            </button>
          )}
        </header>

        {editable && (
          <div className="map-picker__choices" data-testid="map-picker-choices">
            {catalog.state.status === "loading" && (
              <div
                className="map-picker__strip"
                data-testid="map-picker-loading"
                role="status"
                aria-label="Loading maps"
              >
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
                    No ready-made layout fits {playerCount} players. You can still play on a random
                    map.
                  </p>
                )}
                <div className="map-picker__strip">
                  {cards.map((card) => (
                    <MapCard
                      key={card.key}
                      card={card}
                      playerCount={playerCount}
                      disabled={saving}
                      saving={saving && card.selected}
                      onChoose={() => !card.selected && choose(card.choice)}
                    />
                  ))}
                </div>
              </>
            )}
          </div>
        )}

        <div className="map-picker__preview" data-testid="map-picker-preview">
          {preview.state.status === "loading" && (
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
          {preview.state.status === "ready" && (
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
          {preview.refreshing && (
            <span className="map-picker__refreshing" role="status">
              Updating…
            </span>
          )}
        </div>

        <footer className="map-picker__footer">
          <p className="map-picker__summary" data-testid="map-picker-summary">
            {mapSummaryLine(map)}
          </p>
          {editable ? (
            <div className="map-picker__actions">
              {import.meta.env.DEV && (
                <label className="map-picker__dev">
                  Start preset (dev)
                  <select
                    data-testid="dev-start-preset"
                    className="input"
                    value={preset}
                    disabled={saving}
                    onChange={(event) => {
                      setPreset(event.target.value);
                      setPresetTouched(true);
                      // The preset travels with a map choice, so this re-rolls the open slots.
                      if (current) onChoose(current, event.target.value);
                    }}
                  >
                    <option value="">none</option>
                    {DEV_START_PRESETS.map((name) => (
                      <option key={name} value={name}>
                        {name}
                      </option>
                    ))}
                  </select>
                </label>
              )}
              <button
                type="button"
                className="button button--outline"
                data-testid="map-reroll"
                disabled={saving || !current}
                onClick={() => current && choose(current)}
              >
                {saving ? "Saving…" : "Re-roll"}
              </button>
              <button
                type="button"
                className="button button--primary"
                data-testid="map-picker-done"
                onClick={onClose}
              >
                Done
              </button>
            </div>
          ) : null}
        </footer>
      </section>
    </div>
  );
};
