import { useCallback, useEffect, useRef, useState } from "react";
import { decodeMapList, decodeMapPreview } from "../protocol/mapDecode.ts";
import type { MapChoice, MapPreviewDto, MapTemplateSummary } from "../protocol/types.ts";

export type Loadable<T> =
  { status: "loading" } | { status: "error"; message: string } | { status: "ready"; value: T };

async function getJson(url: string): Promise<unknown> {
  const response = await fetch(url);
  if (!response.ok) throw new Error(`${url} failed (${response.status})`);
  return response.json();
}

/** The maps the server offers for a table of this size (buildable ones only); a count of 0 asks for nothing. */
export function useMapCatalog(playerCount: number): {
  state: Loadable<MapTemplateSummary[]>;
  retry: () => void;
} {
  const [state, setState] = useState<Loadable<MapTemplateSummary[]>>({
    status: "loading",
  });
  const [attempt, setAttempt] = useState(0);
  useEffect(() => {
    if (playerCount <= 0) {
      setState({ status: "ready", value: [] });
      return;
    }
    let active = true;
    setState({ status: "loading" });
    getJson(`/api/maps?player_count=${playerCount}`)
      .then((json) => active && setState({ status: "ready", value: decodeMapList(json) }))
      .catch((cause) => active && setState({ status: "error", message: String(cause) }));
    return () => {
      active = false;
    };
  }, [playerCount, attempt]);
  return { state, retry: useCallback(() => setAttempt((n) => n + 1), []) };
}

export type MapPreviewSource =
  | { kind: "lobby"; gameId: string; revision: number }
  | { kind: "card"; choice: MapChoice; playerCount: number; variant?: number };

export function previewUrl(source: MapPreviewSource): string {
  if (source.kind === "lobby")
    return `/api/games/${encodeURIComponent(source.gameId)}/lobby/map-preview`;
  const variant = source.variant ?? 0;
  const alias =
    source.choice.kind === "random" ? "random" : encodeURIComponent(source.choice.alias);
  const count = source.choice.kind === "random" ? `player_count=${source.playerCount}&` : "";
  return `/api/maps/${alias}/preview?${count}variant=${variant}`;
}

const cardCache = new Map<string, MapPreviewDto>();

/** Test hook: card pictures are cached for the session because they never change. */
export function clearMapPreviewCache(): void {
  cardCache.clear();
}

/**
 * A pictured map. Lobby previews are refetched whenever the lobby's `map_revision` changes and
 * keep showing the last picture meanwhile; card previews are cached per choice and variant.
 */
export function useMapPreview(source: MapPreviewSource | null): {
  state: Loadable<MapPreviewDto>;
  refreshing: boolean;
  retry: () => void;
} {
  const url = source ? previewUrl(source) : null;
  const cacheable = source?.kind === "card";
  const key = url === null ? null : source!.kind === "lobby" ? `${url}#${source!.revision}` : url;
  const [state, setState] = useState<Loadable<MapPreviewDto>>(() => {
    const hit = cacheable && url ? cardCache.get(url) : undefined;
    return hit ? { status: "ready", value: hit } : { status: "loading" };
  });
  const [refreshing, setRefreshing] = useState(false);
  const [attempt, setAttempt] = useState(0);
  const last = useRef<MapPreviewDto | null>(null);
  useEffect(() => {
    if (!url) return;
    const hit = cacheable ? cardCache.get(url) : undefined;
    if (hit) {
      last.current = hit;
      setState({ status: "ready", value: hit });
      return;
    }
    let active = true;
    if (last.current && !cacheable) setRefreshing(true);
    else setState({ status: "loading" });
    getJson(url)
      .then((json) => {
        if (!active) return;
        const value = decodeMapPreview(json);
        if (cacheable) cardCache.set(url, value);
        last.current = value;
        setRefreshing(false);
        setState({ status: "ready", value });
      })
      .catch((cause) => {
        if (!active) return;
        setRefreshing(false);
        setState({ status: "error", message: String(cause) });
      });
    return () => {
      active = false;
    };
    // `key` carries the revision for lobby sources.
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [key, attempt]);
  return {
    state,
    refreshing,
    retry: useCallback(() => setAttempt((n) => n + 1), []),
  };
}
