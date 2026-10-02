import {
  BoardView,
  BoardTileView,
  PlanetMetaView,
  PlayerView,
  PendingChoiceDto,
} from "../protocol/types.ts";
import { SEAT_COLORS } from "./playerDisplay.ts";

/** The four closed tech-specialty colours in TI4. */
export type TechSpecialty = "biotic" | "propulsion" | "cybernetic" | "warfare";

export const PLAYER_PALETTE: readonly string[] = SEAT_COLORS;

export const NEUTRAL_COLOR = "#64748b";
export const UNASSIGNED_COLOR = "#94a3b8";

export interface PlayerOwnershipStyle {
  color: string;
  badgeBg: string;
  textContrast: string;
  faction?: string;
  seatIndex: number;
}

export interface PlacedUnitPresentation {
  unitType: string;
  owner: string;
  ownerColor: string;
  planet?: string | null;
  damaged: boolean;
}

export interface PlanetPresentation {
  id: string;
  label: string;
  resources: number;
  influence: number;
  traits: string[];
  techSpecialties: TechSpecialty[];
  legendary: boolean;
  controlledBy: string | null;
  controllerColor: string;
  exhausted: boolean;
  attachments: string[];
  isCandidateTarget: boolean;
  isContextSubject: boolean;
  associatedOptionIds: string[];
}

export interface TilePresentation {
  systemId: string;
  label: string;
  q: number;
  r: number;
  specialArea?: string | null;
  hyperlane?: boolean;
  center: { x: number; y: number };
  points: string;
  innerPoints: string;
  fillColor: string;
  strokeColor: string;
  strokeWidth: number;
  strokeDashArray?: string;
  anomalies: string[];
  anomalyLabel?: string | null;
  wormholes: { kind: string; symbol: string; color: string }[];
  planets: PlanetPresentation[];
  units: PlacedUnitPresentation[];
  totalUnits: number;
  commandTokens: { owner: string; color: string }[];
  isCandidateTarget: boolean;
  isContextSubject: boolean;
  associatedOptionIds: string[];
}

export interface SelectedSystemDetails {
  systemId: string;
  label: string;
  anomalies: string[];
  wormholes: string[];
  specialArea?: string | null;
  isActiveSystem: boolean;
  planets: PlanetPresentation[];
  spaceUnits: PlacedUnitPresentation[];
  planetUnits: Record<string, PlacedUnitPresentation[]>;
  commandTokens: { owner: string; color: string }[];
  availableActions: { optionId: string; label: string; kind?: string }[];
}

export interface MovementVector {
  fromSystemId: string;
  toSystemId: string;
  fromCenter: { x: number; y: number };
  toCenter: { x: number; y: number };
  unitCount: number;
  optionIds: string[];
}

export interface TargetHighlightModel {
  targetableSystemIds: Set<string>;
  targetablePlanetIds: Set<string>;
  contextSubjectSystemId: string | null;
  contextSubjectPlanetId: string | null;
  systemOptionMap: Map<string, string[]>;
  planetOptionMap: Map<string, string[]>;
  hasActiveTargets: boolean;
  isActivationMode: boolean;
  movementVectors: MovementVector[];
}

export interface BoardPresentationModel {
  tiles: TilePresentation[];
  viewBox: string;
  ownershipMap: Map<string, PlayerOwnershipStyle>;
  selectedSystem: SelectedSystemDetails | null;
  targets: TargetHighlightModel;
  activeSystemId: string | null;
}

/**
 * Derives player ownership color from projected seating order position, never by string-matching seat names.
 */
export function getPlayerColor(
  owner: string | null | undefined,
  seatingOrder: readonly string[],
): string {
  if (!owner) return NEUTRAL_COLOR;
  const seatIndex = seatingOrder.indexOf(owner);
  return seatIndex < 0 || seatIndex >= PLAYER_PALETTE.length
    ? UNASSIGNED_COLOR
    : PLAYER_PALETTE[seatIndex];
}

/**
 * Builds a structured ownership palette mapping every player in seating order to a distinct color style.
 */
export function deriveOwnershipPalette(
  seatingOrder: readonly string[],
  players?: readonly PlayerView[],
): Map<string, PlayerOwnershipStyle> {
  const map = new Map<string, PlayerOwnershipStyle>();
  const playerMap = new Map<string, PlayerView>();
  if (players) {
    for (const p of players) {
      playerMap.set(p.id, p);
    }
  }

  seatingOrder.forEach((seat, index) => {
    const color = PLAYER_PALETTE[index] ?? UNASSIGNED_COLOR;
    const player = playerMap.get(seat);
    map.set(seat, {
      color,
      badgeBg: `${color}25`,
      textContrast: "#f8fafc",
      faction: player?.faction,
      seatIndex: index,
    });
  });

  return map;
}

/**
 * Computes flat-topped hex vertices for axial coordinates.
 */
export function getHexPoints(cx: number, cy: number, radius = 70): string {
  const points: string[] = [];
  for (let k = 0; k < 6; k++) {
    const angle = Math.PI / 6 + (Math.PI / 3) * k;
    const px = cx + radius * Math.cos(angle);
    const py = cy + radius * Math.sin(angle);
    points.push(`${px.toFixed(1)},${py.toFixed(1)}`);
  }
  return points.join(" ");
}

export function getInnerPoints(cx: number, cy: number, radius = 64): string {
  return getHexPoints(cx, cy, radius);
}

/**
 * Projects axial hex coordinates into Cartesian SVG space.
 */
export function deriveHexGeometry(
  q: number,
  r: number,
): { center: { x: number; y: number }; points: string; innerPoints: string } {
  const x = 150 * (q + r / 2);
  const y = 130 * r;

  return {
    center: { x, y },
    points: getHexPoints(x, y, 70),
    innerPoints: getInnerPoints(x, y, 64),
  };
}

export function deriveAnomalyVisual(anomalies?: string[]): { color: string; label: string } | null {
  if (!anomalies || anomalies.length === 0) return null;
  const lower = anomalies.map((a) => a.toLowerCase());
  if (lower.some((a) => a.includes("supernova"))) return { color: "#6b271a", label: "SUPERNOVA" };
  if (lower.some((a) => a.includes("gravity rift")))
    return { color: "#38235f", label: "GRAVITY RIFT" };
  if (lower.some((a) => a.includes("nebula"))) return { color: "#173f57", label: "NEBULA" };
  if (lower.some((a) => a.includes("asteroid")))
    return { color: "#3f3b35", label: "ASTEROID FIELD" };
  if (lower.some((a) => a.includes("scar"))) return { color: "#4a2025", label: "CORRUPTED SCAR" };
  return { color: "#1e293b", label: anomalies[0].toUpperCase() };
}

export function deriveWormholeVisual(kind: string): {
  kind: string;
  symbol: string;
  color: string;
} {
  const k = kind.toLowerCase();
  if (k.includes("alpha")) return { kind, color: "#38bdf8", symbol: "α" };
  if (k.includes("beta")) return { kind, color: "#f43f5e", symbol: "β" };
  if (k.includes("gamma")) return { kind, color: "#4ade80", symbol: "γ" };
  if (k.includes("delta")) return { kind, color: "#fb923c", symbol: "δ" };
  return { kind, color: "#94a3b8", symbol: "ω" };
}

/**
 * Derives actor-visible decision targets strictly from structured context and payloads.
 * Zero regular expressions on option IDs.
 */
export function deriveActorTargetHighlights(
  pendingChoice: PendingChoiceDto | null,
  viewerSeat?: string | null,
  board?: BoardView,
): TargetHighlightModel {
  const empty: TargetHighlightModel = {
    targetableSystemIds: new Set(),
    targetablePlanetIds: new Set(),
    contextSubjectSystemId: null,
    contextSubjectPlanetId: null,
    systemOptionMap: new Map(),
    planetOptionMap: new Map(),
    hasActiveTargets: false,
    isActivationMode: false,
    movementVectors: [],
  };

  if (!pendingChoice || !viewerSeat || pendingChoice.actor !== viewerSeat) {
    return empty;
  }

  const targetableSystemIds = new Set<string>();
  const targetablePlanetIds = new Set<string>();
  const systemOptionMap = new Map<string, string[]>();
  const planetOptionMap = new Map<string, string[]>();

  const addSystemOption = (sysId: string, optId: string) => {
    targetableSystemIds.add(sysId);
    const list = systemOptionMap.get(sysId) || [];
    if (!list.includes(optId)) {
      list.push(optId);
      systemOptionMap.set(sysId, list);
    }
  };

  const addPlanetOption = (planetId: string, optId: string) => {
    targetablePlanetIds.add(planetId);
    const list = planetOptionMap.get(planetId) || [];
    if (!list.includes(optId)) {
      list.push(optId);
      planetOptionMap.set(planetId, list);
    }
  };

  let contextSubjectSystemId: string | null = null;
  let contextSubjectPlanetId: string | null = null;

  // 1. Structured DecisionContext target
  const ctxTarget = pendingChoice.context?.target;
  if (ctxTarget) {
    if ("System" in ctxTarget && typeof ctxTarget.System === "string") {
      contextSubjectSystemId = ctxTarget.System;
    } else if ("Planet" in ctxTarget && ctxTarget.Planet) {
      contextSubjectSystemId = ctxTarget.Planet.system;
      contextSubjectPlanetId = ctxTarget.Planet.planet;
    } else if ("Unit" in ctxTarget && ctxTarget.Unit) {
      contextSubjectSystemId = ctxTarget.Unit.system;
    }
  }

  // 2. Structured ChoiceOption payloads (zero regexes)
  for (const opt of pendingChoice.options) {
    const payload = opt.payload;
    const subtype = pendingChoice.context?.subtype;
    const isPaymentPlanet =
      subtype === "pay_resources" ||
      subtype === "pay_influence" ||
      subtype === "vote_exhaust_planet";
    const offeredPlanet =
      isPaymentPlanet && opt.id.startsWith("exhaust|")
        ? opt.id.slice("exhaust|".length).split("|")[0]
        : subtype === "vote_exhaust_planet" && opt.kind === "vote_planet"
          ? opt.id
          : null;
    const planet = offeredPlanet ?? (typeof payload?.planet === "string" ? payload.planet : null);
    const readyOwnedPlanet =
      planet &&
      Object.values(board?.systems ?? {}).some(
        (system) =>
          system.planets[planet]?.controlled_by === pendingChoice.actor &&
          !system.planets[planet].exhausted,
      );
    if (isPaymentPlanet && planet && readyOwnedPlanet) addPlanetOption(planet, opt.id);
    if (payload) {
      if (!isPaymentPlanet && typeof payload.system === "string") {
        addSystemOption(payload.system, opt.id);
      } else if (!isPaymentPlanet && typeof payload.system === "number") {
        addSystemOption(String(payload.system), opt.id);
      }

      if (!isPaymentPlanet && typeof payload.to === "string") {
        addSystemOption(payload.to, opt.id);
      } else if (!isPaymentPlanet && typeof payload.to === "number") {
        addSystemOption(String(payload.to), opt.id);
      }

      if (!isPaymentPlanet && typeof payload.planet === "string") {
        addPlanetOption(payload.planet, opt.id);
      }
    }

    // Semantic kind check for activation where system id is in opt.payload.system or opt.id
    if (opt.kind === "activate") {
      const sys = (payload?.system as string) || opt.id;
      if (sys) {
        addSystemOption(String(sys), opt.id);
      }
    }
  }

  const isActivationMode = Boolean(
    pendingChoice.context?.subtype === "activate_system" ||
    (pendingChoice.options.length > 0 && pendingChoice.options.every((o) => o.kind === "activate")),
  );

  return {
    targetableSystemIds,
    targetablePlanetIds,
    contextSubjectSystemId,
    contextSubjectPlanetId,
    systemOptionMap,
    planetOptionMap,
    hasActiveTargets:
      targetableSystemIds.size > 0 ||
      targetablePlanetIds.size > 0 ||
      contextSubjectSystemId !== null,
    isActivationMode,
    movementVectors: [],
  };
}

/**
 * Extracts structured inspection details for a selected system.
 */
export function deriveSelectedSystemDetails(
  systemId: string,
  board: BoardView,
  seatingOrder: readonly string[],
  _players: readonly PlayerView[],
  pendingChoice: PendingChoiceDto | null,
  viewerSeat?: string | null,
): SelectedSystemDetails | null {
  const dynamicSys = board.systems?.[systemId];
  const staticTile = (board.map_tiles || []).find((t) => t.system_id === systemId);

  if (!dynamicSys && !staticTile) {
    return null;
  }

  const label = staticTile?.label || (systemId === "18" ? "Mecatol Rex" : `#${systemId}`);
  const anomalies = staticTile?.anomalies || [];
  const wormholes = staticTile?.wormholes || [];
  const specialArea = staticTile?.special_area || null;
  const isActiveSystem = board.active_system === systemId;

  // Derive planets
  const dynamicPlanets = dynamicSys?.planets || {};
  const staticPlanets: PlanetMetaView[] = staticTile?.planets || [];
  const planetsMap = new Map<string, PlanetPresentation>();

  // First from static tile
  for (const sp of staticPlanets) {
    const dyn = dynamicPlanets[sp.id];
    planetsMap.set(sp.id, {
      id: sp.id,
      label: sp.label,
      resources: sp.resources,
      influence: sp.influence,
      traits: sp.traits || [],
      techSpecialties: (sp.tech_specialties || []) as TechSpecialty[],
      legendary: Boolean(sp.legendary),
      controlledBy: dyn?.controlled_by || null,
      controllerColor: getPlayerColor(dyn?.controlled_by, seatingOrder),
      exhausted: Boolean(dyn?.exhausted),
      attachments: dyn?.attachments || [],
      isCandidateTarget: false,
      isContextSubject: false,
      associatedOptionIds: [],
    });
  }

  // Next any dynamic planets not in static
  for (const [pId, dyn] of Object.entries(dynamicPlanets)) {
    if (!planetsMap.has(pId)) {
      planetsMap.set(pId, {
        id: pId,
        label: pId,
        resources: 0,
        influence: 0,
        traits: [],
        techSpecialties: [],
        legendary: false,
        controlledBy: dyn.controlled_by || null,
        controllerColor: getPlayerColor(dyn.controlled_by, seatingOrder),
        exhausted: Boolean(dyn.exhausted),
        attachments: dyn.attachments || [],
        isCandidateTarget: false,
        isContextSubject: false,
        associatedOptionIds: [],
      });
    }
  }

  // Derive units
  const units = dynamicSys?.units || [];
  const spaceUnits: PlacedUnitPresentation[] = [];
  const planetUnits: Record<string, PlacedUnitPresentation[]> = {};

  for (const u of units) {
    const unitPres: PlacedUnitPresentation = {
      unitType: u.unit_type,
      owner: u.owner,
      ownerColor: getPlayerColor(u.owner, seatingOrder),
      planet: u.planet,
      damaged: Boolean(u.damaged),
    };
    if (u.planet) {
      if (!planetUnits[u.planet]) planetUnits[u.planet] = [];
      planetUnits[u.planet].push(unitPres);
    } else {
      spaceUnits.push(unitPres);
    }
  }

  // Derive command tokens
  const commandTokens = (dynamicSys?.command_tokens || []).map((owner) => ({
    owner,
    color: getPlayerColor(owner, seatingOrder),
  }));

  // Derive available actions for this system
  const availableActions: { optionId: string; label: string; kind?: string }[] = [];
  if (pendingChoice && viewerSeat && pendingChoice.actor === viewerSeat) {
    for (const opt of pendingChoice.options) {
      const payload = opt.payload;
      const targetsThisSystem =
        (payload?.system !== undefined && String(payload.system) === systemId) ||
        (payload?.to !== undefined && String(payload.to) === systemId) ||
        (opt.kind === "activate" && String(payload?.system || opt.id) === systemId);

      if (targetsThisSystem) {
        availableActions.push({
          optionId: opt.id,
          label: opt.label,
          kind: opt.kind,
        });
      }
    }
  }

  return {
    systemId,
    label,
    anomalies,
    wormholes,
    specialArea,
    isActiveSystem,
    planets: Array.from(planetsMap.values()),
    spaceUnits,
    planetUnits,
    commandTokens,
    availableActions,
  };
}

/**
 * Builds the complete unified board presentation model.
 */
export function buildBoardPresentationModel(
  board: BoardView,
  seatingOrder: readonly string[],
  players: readonly PlayerView[] = [],
  pendingChoice: PendingChoiceDto | null = null,
  viewerSeat?: string | null,
  selectedSystemId?: string | null,
): BoardPresentationModel {
  const ownershipMap = deriveOwnershipPalette(seatingOrder, players);
  const targets = deriveActorTargetHighlights(pendingChoice, viewerSeat, board);
  const systemsMap = board.systems || {};

  // Build unified tile list from map_tiles, or fallback deterministic spiral coordinates
  const rawTiles: BoardTileView[] =
    board.map_tiles && board.map_tiles.length > 0
      ? board.map_tiles
      : Object.keys(systemsMap).map((id, index) => {
          let q = 0;
          let r = 0;
          if (id === "18" || index === 0) {
            q = 0;
            r = 0;
          } else if (id === "34" || index === 1) {
            q = 1;
            r = 0;
          } else {
            q = index;
            r = -index;
          }
          return {
            system_id: id,
            label: id === "18" ? "Mecatol Rex" : `#${id}`,
            q,
            r,
          };
        });

  const regularCenters = rawTiles
    .filter((t) => !t.special_area)
    .map((t) => deriveHexGeometry(t.q, t.r).center);
  const maxY = regularCenters.length ? Math.max(...regularCenters.map((p) => p.y)) : 0;
  const minX = regularCenters.length ? Math.min(...regularCenters.map((p) => p.x)) : 0;
  const fractureTiles = rawTiles.filter((t) => t.special_area === "fracture");
  const fractureStartX = -((fractureTiles.length - 1) / 2) * 150;
  const offMapY = maxY + 190; // 2 hex radii plus a clear gap from the bottom galaxy row

  const tiles: TilePresentation[] = rawTiles.map((tile) => {
    const sysId = tile.system_id;
    const dynamicSystem = systemsMap[sysId];
    const geometry = deriveHexGeometry(tile.q, tile.r);
    const offset =
      tile.special_area === "fracture"
        ? { x: fractureStartX + fractureTiles.indexOf(tile) * 150, y: offMapY }
        : tile.special_area === "nexus"
          ? { x: fractureTiles.length ? fractureStartX - 180 : minX - 180, y: offMapY }
          : geometry.center;
    const { center, points, innerPoints } = tile.special_area
      ? {
          center: offset,
          points: getHexPoints(offset.x, offset.y),
          innerPoints: getInnerPoints(offset.x, offset.y),
        }
      : geometry;

    const anomalyVisual = deriveAnomalyVisual(tile.anomalies);
    let fillColor =
      sysId === "18"
        ? "#1e1b4b"
        : anomalyVisual
          ? anomalyVisual.color
          : tile.hyperlane
            ? "#1e1b4b"
            : "#0f172a";
    if (tile.special_area === "fracture") fillColor = "#112f39";
    if (tile.special_area === "nexus") fillColor = "#29304d";

    // Space control: single player owning space units
    const units = dynamicSystem?.units || [];
    const spaceUnits = units.filter((u) => !u.planet);
    const spaceOwners = Array.from(new Set(spaceUnits.map((u) => u.owner)));
    const singleSpaceOwner = spaceOwners.length === 1 ? spaceOwners[0] : null;
    const dynamicPlanets = dynamicSystem?.planets || {};

    // Border styling
    let strokeColor = "#334155";
    let strokeWidth = 1.5;
    let strokeDashArray: string | undefined;

    if (singleSpaceOwner) {
      strokeColor = "#475569";
      strokeWidth = 1.5;
    }

    if (board.active_system === sysId && !targets.isActivationMode) {
      strokeColor = "#f59e0b";
      strokeWidth = 3;
      strokeDashArray = "6 3";
    }

    // Units
    const placedUnits: PlacedUnitPresentation[] = units.map((u) => ({
      unitType: u.unit_type,
      owner: u.owner,
      ownerColor: getPlayerColor(u.owner, seatingOrder),
      planet: u.planet,
      damaged: Boolean(u.damaged),
    }));

    // Command tokens
    const commandTokens = (dynamicSystem?.command_tokens || []).map((owner) => ({
      owner,
      color: getPlayerColor(owner, seatingOrder),
    }));

    // Target state
    const isCandidateTarget = targets.targetableSystemIds.has(sysId);
    const isContextSubject = targets.contextSubjectSystemId === sysId;
    const associatedOptionIds = targets.systemOptionMap.get(sysId) || [];

    // Planets
    const planets: PlanetPresentation[] = (tile.planets || []).map((sp) => {
      const dyn = dynamicPlanets[sp.id];
      const isPlanetTarget = targets.targetablePlanetIds.has(sp.id);
      const isPlanetSubject = targets.contextSubjectPlanetId === sp.id;
      return {
        id: sp.id,
        label: sp.label,
        resources: sp.resources,
        influence: sp.influence,
        traits: sp.traits || [],
        techSpecialties: (sp.tech_specialties || []) as TechSpecialty[],
        legendary: Boolean(sp.legendary),
        controlledBy: dyn?.controlled_by || null,
        controllerColor: getPlayerColor(dyn?.controlled_by, seatingOrder),
        exhausted: Boolean(dyn?.exhausted),
        attachments: dyn?.attachments || [],
        isCandidateTarget: isPlanetTarget,
        isContextSubject: isPlanetSubject,
        associatedOptionIds: targets.planetOptionMap.get(sp.id) || [],
      };
    });

    // If no static planets but dynamic planets exist
    if (planets.length === 0 && dynamicSystem?.planets) {
      Object.entries(dynamicSystem.planets).forEach(([pId, dyn]) => {
        const isPlanetTarget = targets.targetablePlanetIds.has(pId);
        planets.push({
          id: pId,
          label: pId,
          resources: 0,
          influence: 0,
          traits: [],
          techSpecialties: [],
          legendary: false,
          controlledBy: dyn.controlled_by || null,
          controllerColor: getPlayerColor(dyn.controlled_by, seatingOrder),
          exhausted: Boolean(dyn.exhausted),
          attachments: dyn.attachments || [],
          isCandidateTarget: isPlanetTarget,
          isContextSubject: targets.contextSubjectPlanetId === pId,
          associatedOptionIds: targets.planetOptionMap.get(pId) || [],
        });
      });
    }

    const wormholes = (tile.wormholes || []).map(deriveWormholeVisual);

    return {
      systemId: sysId,
      label: tile.label || (sysId === "18" ? "Mecatol Rex" : `#${sysId}`),
      q: tile.q,
      r: tile.r,
      specialArea: tile.special_area,
      hyperlane: tile.hyperlane,
      center,
      points,
      innerPoints,
      fillColor,
      strokeColor,
      strokeWidth,
      strokeDashArray,
      anomalies: tile.anomalies || [],
      anomalyLabel: anomalyVisual?.label,
      wormholes,
      planets,
      units: placedUnits,
      totalUnits: placedUnits.length,
      commandTokens,
      isCandidateTarget,
      isContextSubject,
      associatedOptionIds,
    };
  });

  const selectedSystem = selectedSystemId
    ? deriveSelectedSystemDetails(
        selectedSystemId,
        board,
        seatingOrder,
        players,
        pendingChoice,
        viewerSeat,
      )
    : null;

  // Derive movement vectors from candidate move options to destination
  const centerMap = new Map<string, { x: number; y: number }>();
  for (const t of tiles) {
    centerMap.set(t.systemId, t.center);
  }

  const destinationSystemId = targets.contextSubjectSystemId || board.active_system || null;
  const movementVectors: MovementVector[] = [];

  if (destinationSystemId && centerMap.has(destinationSystemId) && pendingChoice) {
    const toCenter = centerMap.get(destinationSystemId)!;
    const originOptionMap = new Map<string, string[]>();

    for (const opt of pendingChoice.options) {
      if ((opt.kind === "move" || opt.id.startsWith("move|")) && opt.payload?.origin) {
        const origin = String(opt.payload.origin);
        const list = originOptionMap.get(origin) || [];
        list.push(opt.id);
        originOptionMap.set(origin, list);
      }
    }

    for (const [origId, optIds] of originOptionMap.entries()) {
      if (centerMap.has(origId) && origId !== destinationSystemId) {
        movementVectors.push({
          fromSystemId: origId,
          toSystemId: destinationSystemId,
          fromCenter: centerMap.get(origId)!,
          toCenter,
          unitCount: optIds.length,
          optionIds: optIds,
        });
      }
    }
  }

  targets.movementVectors = movementVectors;

  const xs = tiles.map((tile) => tile.center.x);
  const ys = tiles.map((tile) => tile.center.y);
  const left = xs.length ? Math.min(...xs) - 100 : -600;
  const top = ys.length ? Math.min(...ys) - 100 : -500;
  const width = xs.length ? Math.max(...xs) - left + 100 : 1200;
  const height = ys.length ? Math.max(...ys) - top + 100 : 1000;

  return {
    tiles,
    viewBox: `${left} ${top} ${width} ${height}`,
    ownershipMap,
    selectedSystem,
    targets,
    activeSystemId: board.active_system || null,
  };
}
