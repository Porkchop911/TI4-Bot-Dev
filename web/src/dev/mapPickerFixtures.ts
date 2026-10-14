import type {
  BoardTileView,
  LobbyDto,
  MapChoiceDto,
  MapPreviewDto,
  MapTemplateSummary,
} from "../protocol/types.ts";

/** Axial coordinates of ring `k` around the origin. */
function ring(k: number): [number, number][] {
  if (k === 0) return [[0, 0]];
  const out: [number, number][] = [];
  let q = -k;
  let r = k;
  for (const [dq, dr] of [
    [1, 0],
    [1, -1],
    [0, -1],
    [-1, 0],
    [-1, 1],
    [0, 1],
  ] as [number, number][]) {
    for (let step = 0; step < k; step++) {
      out.push([q, r]);
      q += dq;
      r += dr;
    }
  }
  return out;
}

/** A believable three-ring galaxy for a dev preview; the real tiles come from the server. */
export function fixturePreview(
  playerCount: number,
  alias: string | null = "6pStandard",
): MapPreviewDto {
  const tiles: BoardTileView[] = [];
  const seats: MapPreviewDto["seats"] = [];
  const outer = ring(3);
  const stride = Math.floor(outer.length / playerCount);
  const homeAt = new Map<string, number>();
  for (let seat = 0; seat < playerCount; seat++) {
    const [q, r] = outer[seat * stride];
    homeAt.set(`${q},${r}`, seat);
  }
  let filler = 19;
  for (let k = 0; k <= 3; k++) {
    for (const [q, r] of ring(k)) {
      const seat = homeAt.get(`${q},${r}`);
      if (seat !== undefined) {
        // A home is the place of a seat: no faction sits there before the game starts.
        const id = `seat_${seat + 1}`;
        seats.push({ seat: seat + 1, home_system_id: id });
        tiles.push({ system_id: id, label: `Seat ${seat + 1}`, q, r });
        continue;
      }
      if (k === 0) {
        tiles.push({
          system_id: "18",
          label: "Mecatol Rex",
          q,
          r,
          planets: [{ id: "mr", label: "Mecatol Rex", resources: 1, influence: 6 }],
        });
        continue;
      }
      filler++;
      const id = String(filler);
      const mod = filler % 7;
      tiles.push({
        system_id: id,
        label: `System ${id}`,
        q,
        r,
        anomalies: mod === 0 ? ["nebula"] : undefined,
        wormholes: mod === 1 ? ["alpha"] : undefined,
        planets:
          mod >= 2 && mod <= 5
            ? [
                {
                  id: `${id}-p`,
                  label: `Planet ${id}`,
                  resources: mod % 4,
                  influence: (mod + 1) % 3,
                },
              ]
            : [],
      });
    }
  }
  seats.sort((a, b) => a.seat - b.seat);
  return {
    choice: alias ? { kind: "template", alias } : { kind: "random" },
    player_count: playerCount,
    tiles,
    seats,
  };
}

export const fixtureTemplates = (playerCount: number): MapTemplateSummary[] => [
  {
    alias: `${playerCount}pStandard`,
    author: "Community",
    player_count: playerCount,
    buildable: true,
    systems: 37,
    hyperlanes: false,
    recommended: true,
  },
  {
    alias: `${playerCount}pBeMyNeighbor`,
    author: "Community",
    player_count: playerCount,
    buildable: true,
    systems: 36,
    hyperlanes: false,
    recommended: false,
  },
  {
    alias: `${playerCount}pHyperlanes`,
    author: "Community",
    player_count: playerCount,
    buildable: true,
    systems: 31,
    hyperlanes: true,
    recommended: false,
  },
  {
    alias: `${playerCount}pStandardNucleus`,
    author: "Community",
    player_count: playerCount,
    buildable: true,
    systems: 38,
    hyperlanes: false,
    recommended: false,
  },
];

export const fixtureMapChoice = (alias: string | null, playerCount = 6): MapChoiceDto =>
  alias
    ? {
        kind: "template",
        alias,
        author: "Community",
        systems: 37,
        hyperlanes: false,
        recommended: alias === `${playerCount}pStandard`,
      }
    : { kind: "random", systems: 36, hyperlanes: false, recommended: false };

export function fixtureLobby(
  playerCount = 6,
  alias: string | null = "6pStandard",
  revision = 1,
): LobbyDto {
  return {
    game_id: "gallery-map",
    phase: "lobby",
    lobby_version: 3,
    host_player_id: "host",
    map: fixtureMapChoice(alias, playerCount),
    map_revision: revision,
    slots: Array.from({ length: playerCount }, (_, index) => ({
      slot_id: `slot_${index + 1}`,
      position: index + 1,
      occupant: index < 3 ? (index === 0 ? "host" : `guest_${index}`) : null,
      nickname: index < 3 ? ["Alex", "Blair", "Casey"][index] : null,
      connected: index < 3,
      can_take_over: false,
    })),
  };
}
