import React, { useMemo, useState } from "react";
import { MapPicker } from "../components/MapPicker.tsx";
import { clearMapPreviewCache } from "../hooks/useMapCatalog.ts";
import type { MapChoice } from "../protocol/types.ts";
import { fixtureLobby, fixturePreview, fixtureTemplates } from "./mapPickerFixtures.ts";

export type MapGalleryState = "ready" | "loading" | "error" | "empty" | "saving";

const json = (body: unknown, status = 200) =>
  Promise.resolve(new Response(JSON.stringify(body), { status }));

/** Answers the picker's three requests from fixtures, or fails/hangs on purpose. */
export function installGalleryFetch(state: MapGalleryState, players: number): void {
  const real = window.fetch.bind(window);
  window.fetch = ((input: RequestInfo | URL, init?: RequestInit) => {
    const url = String(input instanceof Request ? input.url : input);
    if (!url.startsWith("/api/maps") && !url.includes("/lobby/map-preview"))
      return real(input, init);
    if (state === "loading") return new Promise<Response>(() => undefined);
    if (state === "error") return json({ error: "gallery: server unavailable" }, 503);
    if (url.startsWith("/api/maps?"))
      return json(state === "empty" ? [] : fixtureTemplates(players));
    const alias = /\/api\/maps\/([^/?]+)\/preview/.exec(url)?.[1];
    return json(
      fixturePreview(players, alias === "random" ? null : (alias ?? `${players}pStandard`)),
    );
  }) as typeof window.fetch;
}

/**
 * Development only: `/dev/map-picker?view=host|readonly&state=ready|loading|error|empty|saving&players=6`
 * shows the host's "Choose your map" sheet or the players' read-only view with fixture data.
 */
export const MapPickerGallery: React.FC = () => {
  const params = new URLSearchParams(window.location.search);
  const view = params.get("view") === "readonly" ? "readonly" : "host";
  const state = (params.get("state") ?? "ready") as MapGalleryState;
  const players = Math.min(8, Math.max(2, Number(params.get("players") ?? 6) || 6));
  useMemo(() => {
    clearMapPreviewCache();
    installGalleryFetch(state, players);
  }, [state, players]);
  const [lobby, setLobby] = useState(() => fixtureLobby(players, `${players}pStandard`));
  const choose = (choice: MapChoice) =>
    setLobby(
      fixtureLobby(
        players,
        choice.kind === "random" ? null : choice.alias,
        (lobby.map_revision ?? 0) + 1,
      ),
    );
  return (
    <main data-testid="map-picker-gallery" style={{ minHeight: "100vh", padding: 24 }}>
      <h1>Map picker</h1>
      <p>Development only. The sheet below is the {view} view.</p>
      <MapPicker
        lobby={lobby}
        editable={view === "host"}
        saving={state === "saving"}
        onChoose={choose}
        onClose={() => undefined}
      />
    </main>
  );
};
