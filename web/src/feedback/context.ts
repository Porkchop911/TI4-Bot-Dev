import { visibilityKind } from "../protocol/eventVisibility.ts";
import type { GameEvent, LobbyDto, PendingChoiceDto, ViewerRole } from "../protocol/types.ts";
import type { SnapshotState } from "../protocol/client.ts";

/**
 * The context attached to a feedback report.
 *
 * It is built only from what the viewer's own client already holds: the viewer-projected snapshot
 * (`view`, never the raw `state`), the viewer's own pending decision and the PUBLIC events. Every
 * field is read explicitly; nothing is spread from a source object, so new fields on the wire
 * cannot leak into a report by accident.
 */

export interface FeedbackSourceData {
  gameId?: string;
  viewer?: ViewerRole;
  snapshot?: SnapshotState | null;
  pendingChoice?: PendingChoiceDto | null;
  /** The roster, for nicknames. Only nicknames, seats and positions are read from it. */
  lobby?: LobbyDto | null;
  events?: GameEvent[];
}

export interface FeedbackEnvironment {
  commit: string;
  serverProtocol: number | null;
  clientProtocol: number;
  viewport: { width: number; height: number; pixelRatio: number };
  userAgent: string;
  now: Date;
  path: string;
  /** The nickname this browser remembers from the lobby (localStorage), if any. */
  storedNickname?: string;
  /** What the sender typed into the optional name field. */
  typedName?: string;
}

export const MAX_RECENT_EVENTS = 20;
const MAX_OPTION_IDS = 60;
const MAX_TEXT = 300;

const clip = (value: unknown): string | undefined =>
  typeof value === "string" ? value.slice(0, MAX_TEXT) : undefined;

export function recentPublicEvents(events: GameEvent[] | undefined, limit = MAX_RECENT_EVENTS) {
  return (events ?? [])
    .filter((event) => visibilityKind(event.visibility) === "public")
    .slice(-limit)
    .map((event) => ({
      id: clip(event.id),
      round: event.round,
      phase: clip(event.phase),
      actor: clip(event.actor),
      action_type: clip(event.action_type),
      stage: clip(event.stage),
      detail: clip(event.detail),
    }));
}

/**
 * Who is reporting: the nicknames from the roster, the seat and faction. Never the seat token
 * (`playerSession`) or any other credential; only the seat id, which every player sees anyway.
 */
export function buildSender(source: FeedbackSourceData, env: FeedbackEnvironment): Record<string, unknown> | null {
  const sender: Record<string, unknown> = {};
  const viewer = source.viewer;
  const slots = source.lobby?.slots ?? [];
  const mine = viewer?.role === "player" ? slots.find((slot) => slot.occupant === viewer.seat) : undefined;
  const nickname = clip(mine?.nickname);
  if (nickname) sender.nickname = nickname;
  if (mine) sender.position = mine.position;
  const stored = clip(env.storedNickname?.trim());
  if (stored && stored !== nickname) sender.lobby_nickname = stored;
  const typed = clip(env.typedName?.trim());
  if (typed) sender.typed_name = typed;
  return Object.keys(sender).length > 0 ? sender : null;
}

export function buildFeedbackContext(
  source: FeedbackSourceData,
  env: FeedbackEnvironment,
  includeGameInfo: boolean,
): Record<string, unknown> {
  const context: Record<string, unknown> = {
    build_commit: env.commit,
    server_protocol_version: env.serverProtocol,
    client_protocol_version: env.clientProtocol,
    viewport: env.viewport,
    user_agent: env.userAgent.slice(0, 400),
    timestamp: env.now.toISOString(),
    path: env.path.replace(/^\/games\/[^/]*/, "/games/:id"),
  };
  const sender = buildSender(source, env);
  if (sender) context.sender = sender;
  if (!includeGameInfo) return context;
  if (source.gameId) context.game_id = source.gameId;
  const viewer = source.viewer;
  if (viewer) {
    context.viewer_role = viewer.role;
    if (viewer.role === "player") context.viewer_seat = viewer.seat;
  }
  const view = source.snapshot?.view;
  if (view) {
    context.round = view.round;
    context.phase = view.phase;
    context.active_player = view.active_player ?? null;
    if (viewer?.role === "player") {
      const me = view.players.find((player) => player.id === viewer.seat);
      if (me) context.faction = me.faction;
    }
  }
  const pending = source.pendingChoice;
  if (pending) {
    context.pending_decision = {
      subtype: clip(pending.context?.subtype) ?? null,
      prompt: clip(pending.prompt) ?? null,
      option_ids: pending.options.slice(0, MAX_OPTION_IDS).map((option) => option.id),
    };
  }
  const slots = source.lobby?.slots ?? [];
  if (view && slots.length > 0) {
    // Seat -> nickname/faction for every seat, so "np gave me a note" can be mapped to a person.
    context.players = view.players.map((player) => {
      const slot = slots.find((entry) => entry.occupant === player.id);
      return {
        seat: player.id,
        position: slot?.position ?? null,
        nickname: clip(slot?.nickname) ?? null,
        bot: slot?.bot ?? null,
        faction: player.faction,
        is_sender: viewer?.role === "player" && viewer.seat === player.id,
      };
    });
  }
  context.recent_public_events = recentPublicEvents(source.events);
  return context;
}

export function currentEnvironment(commit: string, serverProtocol: number | null, clientProtocol: number): FeedbackEnvironment {
  return {
    commit,
    serverProtocol,
    clientProtocol,
    viewport: {
      width: window.innerWidth,
      height: window.innerHeight,
      pixelRatio: window.devicePixelRatio || 1,
    },
    userAgent: navigator.userAgent,
    now: new Date(),
    path: window.location.pathname,
  };
}
