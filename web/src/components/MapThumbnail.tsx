import React from "react";
import type { BoardTileView, MapSeatPreview } from "../protocol/types.ts";
import { deriveHexGeometry } from "../presentation/boardPresentation.ts";
import { homeIdSet, mainTiles, tileKind, type TileKind } from "../presentation/mapPicker.ts";
import { seatStyle } from "../presentation/playerDisplay.ts";

export const TILE_FILL: Record<TileKind, string> = {
  home: "#38bdf8",
  mecatol: "#fbbf24",
  hyperlane: "#6d28d9",
  anomaly: "#b45309",
  wormhole: "#0f766e",
  planets: "#475569",
  empty: "#1e293b",
};

/** One hex per main-map tile, coloured by kind; homes carry their seat colour. */
export const MapThumbnail: React.FC<{
  tiles: readonly BoardTileView[];
  seats: readonly MapSeatPreview[];
  label: string;
}> = ({ tiles, seats, label }) => {
  const main = mainTiles(tiles);
  const homes = homeIdSet(seats);
  const seatOf = new Map(seats.map((seat) => [seat.home_system_id, seat.seat]));
  const placed = main.map((tile) => ({
    tile,
    geometry: deriveHexGeometry(tile.q, tile.r),
  }));
  const xs = placed.map((p) => p.geometry.center.x);
  const ys = placed.map((p) => p.geometry.center.y);
  const pad = 80;
  const left = Math.min(...xs, 0) - pad;
  const top = Math.min(...ys, 0) - pad;
  const width = Math.max(...xs, 0) + pad - left;
  const height = Math.max(...ys, 0) + pad - top;
  return (
    <svg
      className="map-thumbnail"
      viewBox={`${left} ${top} ${width} ${height}`}
      role="img"
      aria-label={label}
      data-testid="map-thumbnail"
    >
      {placed.map(({ tile, geometry }, index) => {
        const kind = tileKind(tile, homes);
        const seat = seatOf.get(tile.system_id);
        return (
          <polygon
            key={`${tile.system_id}-${index}`}
            data-kind={kind}
            points={geometry.points}
            fill={TILE_FILL[kind]}
            stroke={seat ? seatStyle(seat).color : "#0b1220"}
            strokeWidth={seat ? 14 : 6}
          />
        );
      })}
    </svg>
  );
};
