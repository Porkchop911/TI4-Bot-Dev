/**
 * Checks what the server or the engine sends before an app reads it: a `ServerMessage`, and the
 * map answers of the HTTP API. A message that fails throws; the display-only parts of an update
 * (the trigger of a reaction, the auto-resolved notes, the reaction modes) are dropped instead.
 * The errors are for the developer, not for a player.
 */
import type {
  AutoResolvedNote,
  BoardTileView,
  DecisionTrigger,
  InitialSnapshotMsg,
  MapChoice,
  MapChoiceView,
  MapPreview,
  MapTemplateSummary,
  ReactionMode,
  SeatPreview,
  ServerMessage,
  TriggerKind,
  TriggerUnits,
} from "./index.ts";
import { PROTOCOL_VERSION } from "./index.ts";

function isRecord(value: unknown): value is Record<string, unknown> {
  return typeof value === "object" && value !== null && !Array.isArray(value);
}

function isNonNegativeInteger(value: unknown): value is number {
  return typeof value === "number" && Number.isSafeInteger(value) && value >= 0;
}

function fail(message: string): never {
  throw new Error(`Invalid server message: ${message}`);
}

const optionalString = (value: unknown): string | undefined =>
  typeof value === "string" && value.length > 0 ? value : undefined;

/**
 * The reaction trigger of a decision context, or null when absent or malformed. Tolerant on
 * purpose: it is display data, so an unknown or damaged shape degrades the dialog's wording and
 * never fails the message. An unknown `kind` reads as "other".
 */
export function decodeDecisionTrigger(raw: unknown): DecisionTrigger | null {
  if (!isRecord(raw) || typeof raw.event_type !== "string" || raw.event_type.length === 0) {
    return null;
  }
  const units: TriggerUnits[] = [];
  if (Array.isArray(raw.units)) {
    for (const entry of raw.units) {
      if (
        isRecord(entry) &&
        typeof entry.owner === "string" &&
        typeof entry.unit_type === "string" &&
        isNonNegativeInteger(entry.count)
      ) {
        units.push({ owner: entry.owner, unit_type: entry.unit_type, count: entry.count });
      }
    }
  }
  const chain = Array.isArray(raw.chain) ? raw.chain.filter(isNonNegativeInteger) : [];
  return {
    kind: (typeof raw.kind === "string" ? raw.kind : "other") as TriggerKind,
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
  if (!Array.isArray(raw)) {
    return [];
  }
  const notes: AutoResolvedNote[] = [];
  for (const item of raw) {
    if (
      !isRecord(item) ||
      typeof item.id !== "string" ||
      typeof item.prompt !== "string" ||
      typeof item.selected !== "string"
    ) {
      continue;
    }
    const count =
      typeof item.count === "number" && Number.isInteger(item.count) && item.count > 1
        ? item.count
        : undefined;
    const passed = Array.isArray(item.passed)
      ? item.passed.filter((card): card is string => typeof card === "string")
      : [];
    notes.push({
      id: item.id,
      prompt: item.prompt,
      selected: item.selected,
      reason: typeof item.reason === "string" ? item.reason : "",
      ...(count ? { count } : {}),
      ...(passed.length ? { passed } : {}),
    });
  }
  return notes;
}

const REACTION_MODES: readonly unknown[] = [
  "always",
  "never",
  "ownturn",
] satisfies readonly ReactionMode[];

/** The seat's reaction modes; anything that is not a known mode is dropped (settings, not state). */
export function decodeReactionModes(value: unknown): Record<string, ReactionMode> {
  const modes: Record<string, ReactionMode> = {};
  if (!isRecord(value)) {
    return modes;
  }
  for (const [card, mode] of Object.entries(value)) {
    if (REACTION_MODES.includes(mode)) {
      modes[card] = mode as ReactionMode;
    }
  }
  return modes;
}

/** Validates the protocol envelope before an app reads any message of the server or the engine. */
export function decodeServerMessage(value: unknown, expectedGameId: string): ServerMessage {
  if (!isRecord(value) || typeof value.type !== "string") {
    fail("missing message type");
  }
  if (value.protocol_version !== PROTOCOL_VERSION) {
    fail("unsupported protocol version");
  }

  const gameScoped = value.type !== "error" && value.type !== "pong";
  if (gameScoped && value.game_id !== expectedGameId) {
    fail("unexpected game id");
  }

  switch (value.type) {
    case "initial_snapshot":
    case "state_update":
      if (
        !isNonNegativeInteger(value.game_version) ||
        !isRecord(value.view) ||
        !isRecord(value.viewer) ||
        !isRecord(value.turn_status)
      ) {
        fail("invalid snapshot");
      }
      if (
        value.history !== undefined &&
        (!isRecord(value.history) ||
          !isNonNegativeInteger(value.history.cursor) ||
          !isNonNegativeInteger(value.history.redo_count) ||
          (value.history.generation !== undefined &&
            !isNonNegativeInteger(value.history.generation)))
      ) {
        fail("invalid history status");
      }
      if (value.reaction_modes !== undefined) {
        const modes = decodeReactionModes(value.reaction_modes);
        if (Object.keys(modes).length > 0) {
          value.reaction_modes = modes;
        } else {
          delete value.reaction_modes;
        }
      }
      if (value.auto_resolved !== undefined) {
        // Feedback only, so a malformed or unknown shape is dropped rather than failing the update.
        const notes = decodeAutoResolved(value.auto_resolved);
        if (notes.length > 0) {
          value.auto_resolved = notes;
        } else {
          delete value.auto_resolved;
        }
      }
      return value as unknown as ServerMessage;
    case "pending_choice":
      if (
        !isNonNegativeInteger(value.game_version) ||
        typeof value.nonce !== "string" ||
        !isRecord(value.choice) ||
        !isRecord(value.state) ||
        !isRecord(value.galaxy_layout)
      ) {
        fail("invalid pending choice");
      }
      return value as unknown as ServerMessage;
    case "turn_status":
      if (!isNonNegativeInteger(value.game_version) || !isRecord(value.status)) {
        fail("invalid turn status");
      }
      return value as unknown as ServerMessage;
    case "action_accepted":
      if (!isNonNegativeInteger(value.game_version) || typeof value.option_id !== "string") {
        fail("invalid accepted action");
      }
      return value as unknown as ServerMessage;
    case "action_rejected":
      if (!isNonNegativeInteger(value.game_version) || !isRecord(value.reason)) {
        fail("invalid rejected action");
      }
      return value as unknown as ServerMessage;
    case "event": {
      const entry = value.entry;
      if (!isRecord(entry) || typeof entry.id !== "string") {
        fail("invalid event");
      }
      if (entry.batch_id !== undefined && typeof entry.batch_id !== "string") {
        fail("invalid batch ID");
      }
      if (entry.action_id !== undefined && typeof entry.action_id !== "string") {
        fail("invalid action ID");
      }
      if (entry.detail !== undefined && typeof entry.detail !== "string") {
        fail("invalid event detail");
      }
      if (entry.private_detail !== undefined && typeof entry.private_detail !== "string") {
        fail("invalid private event detail");
      }
      if (entry.roll !== undefined) {
        const roll = entry.roll;
        if (
          !isRecord(roll) ||
          !isNonNegativeInteger(roll.index) ||
          typeof roll.reason !== "string" ||
          typeof roll.label !== "string" ||
          !Array.isArray(roll.faces) ||
          !roll.faces.every(isNonNegativeInteger)
        ) {
          fail("invalid dice roll");
        }
      }
      for (const field of ["batch_start_cursor", "batch_end_cursor", "action_start_cursor"]) {
        if (entry[field] !== undefined && !isNonNegativeInteger(entry[field])) {
          fail(`invalid ${field}`);
        }
      }
      if (entry.decision_count !== undefined && !isNonNegativeInteger(entry.decision_count)) {
        fail("invalid event cursor");
      }
      return value as unknown as ServerMessage;
    }
    case "game_over":
      if (!isNonNegativeInteger(value.game_version) || !isRecord(value.final_scores)) {
        fail("invalid game-over message");
      }
      return value as unknown as ServerMessage;
    case "error":
      if (typeof value.kind !== "string" || typeof value.message !== "string") {
        fail("invalid error");
      }
      return value as unknown as ServerMessage;
    case "pong":
      if (!isNonNegativeInteger(value.sequence)) {
        fail("invalid pong");
      }
      return value as unknown as ServerMessage;
    case "draft_outcome": {
      const outcome = value.outcome;
      if (
        typeof value.request_id !== "string" ||
        !isNonNegativeInteger(value.base_version) ||
        !isRecord(outcome) ||
        !isRecord(outcome.update) ||
        !isNonNegativeInteger(outcome.consumed) ||
        !isRecord(outcome.stop) ||
        typeof outcome.stop.kind !== "string"
      ) {
        fail("invalid draft outcome");
      }
      return value as unknown as ServerMessage;
    }
    case "draft_applied": {
      const stopped = value.stopped;
      if (
        typeof value.nonce !== "string" ||
        (stopped !== undefined &&
          (!isRecord(stopped) ||
            !isNonNegativeInteger(stopped.applied) ||
            !Array.isArray(stopped.remaining) ||
            typeof stopped.interrupted !== "boolean"))
      ) {
        fail("invalid applied draft");
      }
      return value as unknown as ServerMessage;
    }
    case "seat_staged":
      if (
        !isNonNegativeInteger(value.revision) ||
        !isRecord(value.slots) ||
        (value.writer !== undefined && !isNonNegativeInteger(value.writer)) ||
        (value.connection !== undefined && !isNonNegativeInteger(value.connection))
      ) {
        fail("invalid staged work");
      }
      return value as unknown as ServerMessage;
    case "reaction_intent_state":
      if (
        !Array.isArray(value.triggers) ||
        !value.triggers.every((id) => typeof id === "string") ||
        typeof value.eligible !== "boolean" ||
        typeof value.budget_used_up !== "boolean" ||
        typeof value.holding !== "boolean" ||
        (value.locked_until_round !== undefined && !isNonNegativeInteger(value.locked_until_round))
      ) {
        fail("invalid reaction intent state");
      }
      return value as unknown as ServerMessage;
    case "secondary_preview": {
      const outcome = value.outcome;
      if (
        !isNonNegativeInteger(value.request_id) ||
        !isNonNegativeInteger(value.as_of_version) ||
        !isRecord(outcome) ||
        (outcome.result !== "preview" && outcome.result !== "refused")
      ) {
        fail("invalid secondary preview");
      }
      if (outcome.result === "refused") {
        if (typeof outcome.reason !== "string") {
          fail("invalid secondary preview refusal");
        }
      } else {
        const preview = outcome.preview;
        if (!isRecord(preview) || typeof preview.status !== "string") {
          fail("invalid secondary preview body");
        }
        if (preview.status === "question") {
          const choice = preview.choice;
          if (
            !isRecord(choice) ||
            typeof choice.player !== "string" ||
            typeof choice.prompt !== "string" ||
            !Array.isArray(choice.options)
          ) {
            fail("invalid previewed choice");
          }
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
      ) {
        fail("invalid reach answer");
      }
      return value as unknown as ServerMessage;
    default:
      return fail("unknown message type");
  }
}

export function decodeInitialSnapshot(value: unknown, expectedGameId: string): InitialSnapshotMsg {
  const message = decodeServerMessage(value, expectedGameId);
  if (message.type !== "initial_snapshot") {
    fail("expected initial snapshot");
  }
  return message;
}

/** Whether a message is of an older game version than the one shown. */
export function isStaleServerMessage(message: ServerMessage, currentVersion: number): boolean {
  const version =
    "game_version" in message
      ? message.game_version
      : message.type === "event"
        ? message.entry.version
        : undefined;
  return version !== undefined && version < currentVersion;
}

// ---- The maps of the HTTP API ------------------------------------------------------------------

function failMap(message: string): never {
  throw new Error(`Invalid map response: ${message}`);
}

const text = (value: unknown, max = 128): value is string =>
  typeof value === "string" && value.length > 0 && value.length <= max;

/** The public description of a lobby's map choice. */
export function decodeMapChoice(value: unknown): MapChoiceView {
  if (
    !isRecord(value) ||
    (value.kind !== "template" && value.kind !== "random") ||
    (value.alias !== undefined && !text(value.alias)) ||
    (value.name !== undefined && !text(value.name)) ||
    (value.author !== undefined && typeof value.author !== "string") ||
    !isNonNegativeInteger(value.systems) ||
    typeof value.hyperlanes !== "boolean" ||
    typeof value.recommended !== "boolean"
  ) {
    failMap("map choice");
  }
  if ((value.kind === "template") !== (value.alias !== undefined)) {
    failMap("map choice alias");
  }
  return {
    kind: value.kind,
    ...(value.alias === undefined ? {} : { alias: value.alias as string }),
    ...(value.name === undefined ? {} : { name: value.name as string }),
    ...(value.author === undefined ? {} : { author: value.author as string }),
    systems: value.systems,
    hyperlanes: value.hyperlanes,
    recommended: value.recommended,
  };
}

/** `GET /api/maps`; entries that do not build are dropped, so the picker only offers real maps. */
export function decodeMapList(value: unknown): MapTemplateSummary[] {
  if (!Array.isArray(value)) {
    failMap("map list");
  }
  return value
    .map((entry): MapTemplateSummary => {
      if (
        !isRecord(entry) ||
        !text(entry.alias) ||
        typeof entry.author !== "string" ||
        (entry.name !== undefined && !text(entry.name)) ||
        !isNonNegativeInteger(entry.player_count)
      ) {
        failMap("map list entry");
      }
      return {
        alias: entry.alias,
        ...(entry.name === undefined ? {} : { name: entry.name as string }),
        author: entry.author,
        player_count: entry.player_count,
        buildable: entry.buildable === true,
        systems: isNonNegativeInteger(entry.systems) ? entry.systems : 0,
        hyperlanes: entry.hyperlanes === true,
        recommended: entry.recommended === true,
      };
    })
    .filter((entry) => entry.buildable);
}

function decodeTile(entry: unknown): BoardTileView {
  if (
    !isRecord(entry) ||
    !text(entry.system_id) ||
    typeof entry.label !== "string" ||
    typeof entry.q !== "number" ||
    typeof entry.r !== "number" ||
    !Number.isInteger(entry.q) ||
    !Number.isInteger(entry.r)
  ) {
    failMap("map tile");
  }
  return entry as unknown as BoardTileView;
}

function decodeSeat(entry: unknown): SeatPreview {
  if (!isRecord(entry) || !isNonNegativeInteger(entry.seat) || !text(entry.home_system_id)) {
    failMap("map seat");
  }
  return {
    seat: entry.seat,
    home_system_id: entry.home_system_id,
  };
}

/** `GET /api/maps/preview`: the tiles of a map choice and where each seat sits. */
export function decodeMapPreview(value: unknown): MapPreview {
  if (!isRecord(value) || !isRecord(value.choice) || !isNonNegativeInteger(value.player_count)) {
    failMap("map preview");
  }
  if (!Array.isArray(value.tiles) || value.tiles.length > 400) {
    failMap("map preview tiles");
  }
  if (!Array.isArray(value.seats) || value.seats.length > 8) {
    failMap("map preview seats");
  }
  const choice: MapChoice =
    value.choice.kind === "template" && text(value.choice.alias)
      ? { kind: "template", alias: value.choice.alias }
      : value.choice.kind === "random"
        ? { kind: "random" }
        : failMap("map preview choice");
  return {
    choice,
    player_count: value.player_count,
    tiles: value.tiles.map(decodeTile),
    seats: value.seats.map(decodeSeat),
  };
}
