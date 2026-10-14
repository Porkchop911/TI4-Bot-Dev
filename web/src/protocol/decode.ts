import {
  JoinResponse,
  CreateGameResponse,
  AutoResolvedNote,
  DecisionTriggerDto,
  InitialSnapshotMsg,
  LobbyDto,
  PROTOCOL_VERSION,
  ServerMessage,
  TriggerKindDto,
  TriggerUnitsDto,
} from "./types.ts";
import { isStrategyCardSetId } from "../presentation/strategyCardSet.ts";
import { validNickname } from "./nickname.ts";
import { decodeMapChoice } from "./mapDecode.ts";

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
    bot_service_enabled: Boolean(value.bot_service_enabled),
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

/**
 * The reaction trigger of a decision context, or null when absent or malformed. Tolerant on
 * purpose: it is display data, so an unknown or damaged shape degrades the dialog's wording and
 * never fails the message. An unknown `kind` reads as "other".
 */
export function decodeDecisionTrigger(raw: unknown): DecisionTriggerDto | null {
  if (!isRecord(raw) || typeof raw.event_type !== "string" || raw.event_type.length === 0)
    return null;
  const units: TriggerUnitsDto[] = [];
  if (Array.isArray(raw.units)) {
    for (const entry of raw.units) {
      if (
        isRecord(entry) &&
        typeof entry.owner === "string" &&
        typeof entry.unit_type === "string" &&
        isNonNegativeInteger(entry.count)
      )
        units.push({ owner: entry.owner, unit_type: entry.unit_type, count: entry.count });
    }
  }
  const chain = Array.isArray(raw.chain) ? raw.chain.filter(isNonNegativeInteger) : [];
  return {
    kind: (typeof raw.kind === "string" ? raw.kind : "other") as TriggerKindDto,
    event_type: raw.event_type,
    event_id: isNonNegativeInteger(raw.event_id) ? raw.event_id : 0,
    relation: raw.relation === "when" ? "when" : "after",
    ...(optionalString(raw.actor) ? { actor: optionalString(raw.actor) } : {}),
    ...(optionalString(raw.subject) ? { subject: optionalString(raw.subject) } : {}),
    ...(optionalString(raw.card) ? { card: optionalString(raw.card) } : {}),
    ...(optionalString(raw.agenda) ? { agenda: optionalString(raw.agenda) } : {}),
    ...(optionalString(raw.system) ? { system: optionalString(raw.system) } : {}),
    ...(optionalString(raw.planet) ? { planet: optionalString(raw.planet) } : {}),
    ...(units.length ? { units } : {}),
    ...(isNonNegativeInteger(raw.hits) ? { hits: raw.hits } : {}),
    ...(chain.length ? { chain } : {}),
  };
}

/** The well-formed auto-resolved notes in a state update; anything else is ignored. */
export function decodeAutoResolved(raw: unknown): AutoResolvedNote[] {
  if (!Array.isArray(raw)) return [];
  const notes: AutoResolvedNote[] = [];
  for (const item of raw) {
    if (
      !isRecord(item) ||
      typeof item.id !== "string" ||
      typeof item.prompt !== "string" ||
      typeof item.selected !== "string"
    )
      continue;
    const count = typeof item.count === "number" && Number.isInteger(item.count) && item.count > 1 ? item.count : undefined;
    notes.push({
      id: item.id,
      prompt: item.prompt,
      selected: item.selected,
      reason: typeof item.reason === "string" ? item.reason : "",
      ...(count ? { count } : {}),
    });
  }
  return notes;
}

/** The seat's reaction modes; anything that is not a known mode is dropped (settings, not state). */
export function decodeReactionModes(value: unknown): Record<string, "always" | "never"> {
  const modes: Record<string, "always" | "never"> = {};
  if (!isRecord(value)) return modes;
  for (const [card, mode] of Object.entries(value)) {
    if (mode === "always" || mode === "never") modes[card] = mode;
  }
  return modes;
}

/** Validates the protocol envelope before React consumes any network payload. */
export function decodeServerMessage(value: unknown, expectedGameId: string): ServerMessage {
  if (!isRecord(value) || typeof value.type !== "string") fail("missing message type");
  if (value.protocol_version !== PROTOCOL_VERSION) fail("unsupported protocol version");

  const gameScoped = value.type !== "error" && value.type !== "pong";
  if (gameScoped && value.game_id !== expectedGameId) fail("unexpected game id");

  switch (value.type) {
    case "initial_snapshot":
    case "state_update":
      if (
        !isNonNegativeInteger(value.game_version) ||
        !isRecord(value.view) ||
        !isRecord(value.viewer) ||
        !isRecord(value.turn_status)
      )
        fail("invalid snapshot");
      if (
        value.history !== undefined &&
        (!isRecord(value.history) ||
          !isNonNegativeInteger(value.history.cursor) ||
          !isNonNegativeInteger(value.history.redo_count) ||
          (value.history.generation !== undefined &&
            !isNonNegativeInteger(value.history.generation)))
      )
        fail("invalid history status");
      if (value.reaction_modes !== undefined) {
        const modes = decodeReactionModes(value.reaction_modes);
        if (Object.keys(modes).length > 0) value.reaction_modes = modes;
        else delete value.reaction_modes;
      }
      if (value.auto_resolved !== undefined) {
        // Feedback only, so a malformed or unknown shape is dropped rather than failing the update.
        const notes = decodeAutoResolved(value.auto_resolved);
        if (notes.length > 0) value.auto_resolved = notes;
        else delete value.auto_resolved;
      }
      return value as unknown as ServerMessage;
    case "pending_choice":
      if (
        !isNonNegativeInteger(value.game_version) ||
        typeof value.nonce !== "string" ||
        !isRecord(value.choice) ||
        !isRecord(value.state) ||
        !isRecord(value.galaxy_layout)
      )
        fail("invalid pending choice");
      return value as unknown as ServerMessage;
    case "turn_status":
      if (!isNonNegativeInteger(value.game_version) || !isRecord(value.status))
        fail("invalid turn status");
      return value as unknown as ServerMessage;
    case "action_accepted":
      if (!isNonNegativeInteger(value.game_version) || typeof value.option_id !== "string")
        fail("invalid accepted action");
      return value as unknown as ServerMessage;
    case "action_rejected":
      if (!isNonNegativeInteger(value.game_version) || !isRecord(value.reason))
        fail("invalid rejected action");
      return value as unknown as ServerMessage;
    case "event":
      if (!isRecord(value.entry) || typeof value.entry.id !== "string") fail("invalid event");
      if (value.entry.batch_id !== undefined && typeof value.entry.batch_id !== "string")
        fail("invalid batch ID");
      if (value.entry.action_id !== undefined && typeof value.entry.action_id !== "string")
        fail("invalid action ID");
      if (value.entry.detail !== undefined && typeof value.entry.detail !== "string")
        fail("invalid event detail");
      if (
        value.entry.private_detail !== undefined &&
        typeof value.entry.private_detail !== "string"
      )
        fail("invalid private event detail");
      if (value.entry.roll !== undefined) {
        const roll = value.entry.roll;
        if (
          !isRecord(roll) ||
          !isNonNegativeInteger(roll.index) ||
          typeof roll.reason !== "string" ||
          typeof roll.label !== "string" ||
          !Array.isArray(roll.faces) ||
          !roll.faces.every(isNonNegativeInteger)
        )
          fail("invalid dice roll");
      }
      for (const field of ["batch_start_cursor", "batch_end_cursor", "action_start_cursor"]) {
        if (value.entry[field] !== undefined && !isNonNegativeInteger(value.entry[field]))
          fail(`invalid ${field}`);
      }
      if (
        value.entry.decision_count !== undefined &&
        !isNonNegativeInteger(value.entry.decision_count)
      )
        fail("invalid event cursor");
      return value as unknown as ServerMessage;
    case "game_over":
      if (!isNonNegativeInteger(value.game_version) || !isRecord(value.final_scores))
        fail("invalid game-over message");
      return value as unknown as ServerMessage;
    case "error":
      if (typeof value.kind !== "string" || typeof value.message !== "string")
        fail("invalid error");
      return value as unknown as ServerMessage;
    case "pong":
      if (!isNonNegativeInteger(value.sequence)) fail("invalid pong");
      return value as unknown as ServerMessage;
    case "reaction_intent_state":
      if (
        !Array.isArray(value.triggers) ||
        !value.triggers.every((id) => typeof id === "string") ||
        typeof value.eligible !== "boolean" ||
        typeof value.budget_used_up !== "boolean" ||
        typeof value.holding !== "boolean" ||
        (value.locked_until_round !== undefined && !isNonNegativeInteger(value.locked_until_round))
      )
        fail("invalid reaction intent state");
      return value as unknown as ServerMessage;
    case "secondary_preview": {
      const outcome = value.outcome;
      if (
        !isNonNegativeInteger(value.request_id) ||
        !isNonNegativeInteger(value.as_of_version) ||
        !isRecord(outcome) ||
        (outcome.result !== "preview" && outcome.result !== "refused")
      )
        fail("invalid secondary preview");
      if (outcome.result === "refused") {
        if (typeof outcome.reason !== "string") fail("invalid secondary preview refusal");
      } else {
        const preview = outcome.preview;
        if (!isRecord(preview) || typeof preview.status !== "string")
          fail("invalid secondary preview body");
        if (preview.status === "question") {
          const choice = preview.choice;
          if (
            !isRecord(choice) ||
            typeof choice.player !== "string" ||
            typeof choice.prompt !== "string" ||
            !Array.isArray(choice.options)
          )
            fail("invalid previewed choice");
        }
      }
      return value as unknown as ServerMessage;
    }
    case "reach_into":
      if (
        !isNonNegativeInteger(value.request_id) ||
        !isNonNegativeInteger(value.as_of_version) ||
        typeof value.system_id !== "string" ||
        !Array.isArray(value.origins) ||
        !value.origins.every(
          (origin) =>
            isRecord(origin) &&
            typeof origin.owner === "string" &&
            typeof origin.origin === "string" &&
            Array.isArray(origin.ships) &&
            origin.ships.every(
              (ship) =>
                isRecord(ship) &&
                typeof ship.type_id === "string" &&
                isNonNegativeInteger(ship.count),
            ),
        )
      )
        fail("invalid reach answer");
      return value as unknown as ServerMessage;
    default:
      return fail("unknown message type");
  }
}

export function decodeInitialSnapshot(value: unknown, expectedGameId: string): InitialSnapshotMsg {
  const message = decodeServerMessage(value, expectedGameId);
  if (message.type !== "initial_snapshot") fail("expected initial snapshot");
  return message;
}

export function isStaleServerMessage(message: ServerMessage, currentVersion: number): boolean {
  const version =
    "game_version" in message
      ? message.game_version
      : message.type === "event"
        ? message.entry.version
        : undefined;
  return version !== undefined && version < currentVersion;
}
