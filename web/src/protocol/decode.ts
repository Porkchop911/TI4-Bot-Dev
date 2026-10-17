import { JoinResponse, CreateGameResponse, LobbyDto } from "./types.ts";
import type { ActiveEffectDto, EffectExpiryDto, EffectPreviewDto, TriggerUnitsDto } from "./types.ts";
import { decodeMapChoice } from "@ti4/shared/wire/decode.ts";
import { isStrategyCardSetId } from "../presentation/strategyCardSet.ts";
import { validNickname } from "./nickname.ts";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNonNegativeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function fail(message: string): never {
  throw new Error(`Invalid server message: ${message}`);
}

function isBoundedString(value: unknown, maxLength: number): value is string {
  return typeof value === "string" && value.length > 0 && value.length <= maxLength;
}

const isGameId = (value: unknown): value is string => isBoundedString(value, 64);
const isSlotId = (value: unknown): value is string => isBoundedString(value, 64);
const isPlayerId = (value: unknown): value is string => isBoundedString(value, 128);
const isPlayerSession = (value: unknown): value is string => isBoundedString(value, 128);

export function decodeLobby(value: unknown, expectedGameId: string): LobbyDto {
  if (!isRecord(value) || value.game_id !== expectedGameId) fail("invalid lobby game id");
  if (value.phase !== "lobby" && value.phase !== "running") fail("invalid lobby phase");
  if (!isNonNegativeInteger(value.lobby_version) || !isPlayerId(value.host_player_id))
    fail("invalid lobby metadata");
  if (!Array.isArray(value.slots) || value.slots.length < 2 || value.slots.length > 8)
    fail("invalid lobby slots");
  const slots = new Set<string>();
  const players = new Set<string>();
  for (const [index, entry] of value.slots.entries()) {
    if (
      !isRecord(entry) ||
      !isSlotId(entry.slot_id) ||
      slots.has(entry.slot_id) ||
      entry.position !== index + 1 ||
      (entry.occupant !== null && !isPlayerId(entry.occupant)) ||
      typeof entry.connected !== "boolean" ||
      typeof entry.can_take_over !== "boolean" ||
      (entry.occupant === null
        ? entry.nickname !== null || entry.connected || entry.can_take_over
        : !validNickname(entry.nickname)) ||
      (entry.occupant !== null && players.has(entry.occupant))
    )
      fail("invalid lobby slot");
    slots.add(entry.slot_id);
    if (entry.occupant !== null) players.add(entry.occupant);
  }
  if (!players.has(value.host_player_id)) fail("unknown lobby host");
  return {
    game_id: value.game_id,
    phase: value.phase,
    lobby_version: value.lobby_version,
    host_player_id: value.host_player_id,
    slots: value.slots.map((entry: Record<string, unknown>) => ({
      slot_id: entry.slot_id,
      position: entry.position,
      occupant: entry.occupant,
      nickname: entry.nickname,
      connected: entry.connected,
      can_take_over: entry.can_take_over,
      ...(typeof entry.bot === "string" && entry.occupant !== null ? { bot: entry.bot } : {}),
      ...(typeof entry.faction === "string" && entry.occupant !== null
        ? { faction: entry.faction }
        : {}),
    })),
    bot_kinds: Array.isArray(value.bot_kinds)
      ? value.bot_kinds.filter((kind: unknown): kind is string => typeof kind === "string")
      : [],
    ...(Array.isArray(value.factions)
      ? {
          factions: value.factions.filter(
            (faction: unknown): faction is string => typeof faction === "string",
          ),
        }
      : {}),
    ...(isStrategyCardSetId(value.strategy_card_set)
      ? { strategy_card_set: value.strategy_card_set }
      : {}),
    ...(value.map === undefined
      ? {}
      : {
          map: decodeMapChoice(value.map),
          map_revision: isNonNegativeInteger(value.map_revision) ? value.map_revision : 0,
        }),
  } as LobbyDto;
}

export function decodeCreateGameResponse(value: unknown): CreateGameResponse {
  if (
    !isRecord(value) ||
    !isGameId(value.game_id) ||
    !isPlayerSession(value.player_session) ||
    !isRecord(value.player) ||
    !isPlayerId(value.player.id)
  )
    fail("invalid game creation response");
  const lobby = decodeLobby(value.lobby, value.game_id);
  const player = value.player as Record<string, unknown>;
  if (!lobby.slots.some((slot) => slot.occupant === player.id)) fail("unknown player");
  return {
    game_id: value.game_id,
    player_session: value.player_session,
    player: { id: player.id as string },
    lobby,
  };
}

export function decodeJoinResponse(value: unknown, expectedGameId: string): JoinResponse {
  if (
    !isRecord(value) ||
    !isRecord(value.player) ||
    !isPlayerId(value.player.id) ||
    (value.player_session !== null &&
      value.player_session !== undefined &&
      !isPlayerSession(value.player_session))
  )
    fail("invalid join response");
  const lobby = decodeLobby(value.lobby, expectedGameId);
  const player = value.player as Record<string, unknown>;
  if (!lobby.slots.some((slot) => slot.occupant === player.id)) fail("unknown player");
  return {
    player_session: value.player_session ?? undefined,
    player: { id: player.id as string },
    lobby,
  };
}

const optionalString = (value: unknown): string | undefined =>
  typeof value === "string" && value.length > 0 ? value : undefined;

// Our additions (action card reaction previews, lingering effects), kept beside the shared decoder.
const strings = (value: unknown): string[] =>
  Array.isArray(value) ? value.filter((entry): entry is string => typeof entry === "string" && entry.length > 0) : [];

const finiteNumber = (value: unknown): number | undefined =>
  typeof value === "number" && Number.isFinite(value) ? value : undefined;

/**
 * The well-formed effect previews of a reaction decision; a damaged entry is dropped. Display data:
 * never fails the message.
 */
export function decodeEffectPreviews(raw: unknown): EffectPreviewDto[] {
  if (!Array.isArray(raw)) return [];
  const previews: EffectPreviewDto[] = [];
  for (const entry of raw) {
    if (
      !isRecord(entry) ||
      typeof entry.option !== "string" ||
      typeof entry.card !== "string" ||
      typeof entry.effect !== "string"
    )
      continue;
    const units: TriggerUnitsDto[] = [];
    if (Array.isArray(entry.units)) {
      for (const unit of entry.units) {
        if (
          isRecord(unit) &&
          typeof unit.owner === "string" &&
          typeof unit.unit_type === "string" &&
          isNonNegativeInteger(unit.count)
        )
          units.push({ owner: unit.owner, unit_type: unit.unit_type, count: unit.count });
      }
    }
    const seats = strings(entry.seats);
    const planets = strings(entry.planets);
    const refs = strings(entry.refs);
    const amount = finiteNumber(entry.amount);
    const of = finiteNumber(entry.of);
    previews.push({
      option: entry.option,
      card: entry.card,
      effect: entry.effect,
      ...(seats.length ? { seats } : {}),
      ...(optionalString(entry.system) ? { system: optionalString(entry.system) } : {}),
      ...(planets.length ? { planets } : {}),
      ...(units.length ? { units } : {}),
      ...(amount !== undefined ? { amount } : {}),
      ...(of !== undefined ? { of } : {}),
      ...(refs.length ? { refs } : {}),
      ...(optionalString(entry.fizzles) ? { fizzles: optionalString(entry.fizzles) } : {}),
    });
  }
  return previews;
}

const EXPIRIES: readonly EffectExpiryDto[] = [
  "combat_round",
  "tactical_action",
  "invasion",
  "agenda",
  "next_action",
  "negotiation",
  "next_strategy_phase",
];

/** The well-formed lingering effects of a view (`GameView.active_effects`); absent is empty. */
export function decodeActiveEffects(raw: unknown): ActiveEffectDto[] {
  if (!Array.isArray(raw)) return [];
  const effects: ActiveEffectDto[] = [];
  for (const entry of raw) {
    if (
      !isRecord(entry) ||
      typeof entry.seat !== "string" ||
      typeof entry.effect !== "string" ||
      !EXPIRIES.includes(entry.until as EffectExpiryDto)
    )
      continue;
    const amount = finiteNumber(entry.amount);
    effects.push({
      seat: entry.seat,
      effect: entry.effect,
      until: entry.until as EffectExpiryDto,
      ...(optionalString(entry.card) ? { card: optionalString(entry.card) } : {}),
      ...(isNonNegativeInteger(entry.copies) && entry.copies > 1 ? { copies: entry.copies } : {}),
      ...(amount !== undefined ? { amount } : {}),
      ...(optionalString(entry.other) ? { other: optionalString(entry.other) } : {}),
      ...(optionalString(entry.system) ? { system: optionalString(entry.system) } : {}),
    });
  }
  return effects;
}

/** The well-formed auto-resolved notes in a state update; anything else is ignored. */
