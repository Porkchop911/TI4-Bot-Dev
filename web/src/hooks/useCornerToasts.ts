import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import type { AutoResolveNotification } from "../components/AutoResolveToast.tsx";
import type { AutoResolvedNote, BoardView, GameEvent, PendingChoiceDto, PlayerView, TableView } from "../protocol/types.ts";
import { imperialNoScoreNotice, ownImperialPlay } from "../presentation/publicScoringBlock.ts";
import {
  ActionToast,
  actionToastFromEvent,
  isCardOrScore,
  actionToastText,
  autoResolvedToast,
  mergeActionToast,
  victoryPointToasts,
} from "../presentation/actionToasts.ts";
import { mergeRollToasts, rollToasts, type RollToast } from "../presentation/rollToasts.ts";
import { explorationToasts, type ExplorationRecordDto } from "../presentation/explorationToasts.ts";
import { breakthroughToasts } from "../presentation/breakthroughs.ts";
import { leaderUnlockToasts, readLeaderStatuses } from "../presentation/leaderUnlocks.ts";
import { agendaToasts, type AgendaRecordDto, type AgendaToast } from "../presentation/agendaToasts.ts";
import { relicToasts, type RelicGainDto, type RelicToast } from "../presentation/relicToasts.ts";
import { buildTurnRecap, trackTurns, type OpenTurn, type TurnRecap } from "../presentation/turnRecap.ts";
import { isTurnMenuChoice } from "../presentation/turnBar.ts";
import { trackInvasion, type InvasionToast, type InvasionTracker } from "../presentation/invasionToasts.ts";
import { usePlayerIdentity } from "../presentation/PlayerIdentity.tsx";
import { recordToasts, type ToastHistoryEntry } from "../presentation/toastHistory.ts";
import { useTurnRecapSetting } from "./useTurnRecapSetting.ts";
import { useAutoResolveToasts } from "./useAutoResolveToasts.ts";
import { useToastMute } from "./useToastMute.ts";

export interface CornerToastsInput {
  /** The public event log as the session delivers it. */
  events: readonly GameEvent[];
  players?: readonly (Pick<PlayerView, "id" | "victory_points"> & Partial<Pick<PlayerView, "leaders">>)[];
  /** The viewer's seat; absent for spectators, who get every player's actions. */
  viewerSeat?: string | null;
  pendingChoice?: PendingChoiceDto | null;
  /** The server's notes for decisions it settled for this seat (state updates only). */
  autoResolved?: readonly AutoResolvedNote[];
  /** The history generation of the session; a new one replaces the timeline (undo/redo, rewind). */
  historyGeneration?: number;
  /** Decisions this client answered for the viewer (lone strategic action / activation). */
  localNotes?: readonly { id: string; text: string; prompt?: string }[];
  /** The board of the latest snapshot: bombardment, Space Cannon Defense and invasion results are read from it. */
  board?: BoardView | null;
  /** The engine's public exploration log: every finished exploration toasts, for every seat. */
  explorations?: readonly ExplorationRecordDto[];
  /** Each seat's earned breakthrough alias; a new one toasts for every seat. */
  breakthroughs?: Readonly<Record<string, string | null>>;
  /** The engine's public agenda log: every finished vote toasts, for every seat (voters too). */
  agendas?: readonly AgendaRecordDto[];
  /** The engine's public relic log: every relic a player gains toasts, for every seat. */
  relics?: readonly RelicGainDto[];
  /** The table of the latest snapshot: why the viewer's Imperial scored nothing is read from it. */
  table?: TableView | null;
  /** False until the first snapshot arrived: what is already in the log is history, not news. */
  ready: boolean;
}

const MAX_RECAPS = 2;
const MAX_INVASION_TOASTS = 3;
const MAX_AGENDA_TOASTS = 2;
const MAX_ROLL_TOASTS = 3;
const MAX_RELIC_TOASTS = 3;
const RELIC_LINGER_MS = 10000;

// What a player gains from an exploration or a breakthrough is read, not glanced at: it stays longer.
const LINGERING_CATEGORIES: ReadonlySet<ActionToast["category"]> = new Set(["explore", "breakthrough"]);
const LINGER_MS = 10000;

const INVASION_TAGS: Record<InvasionToast["kind"], string> = {
  bombardment: "Bombardment",
  space_cannon_defense: "Space Cannon",
  summary: "Invasion",
};

type Shown = ActionToast & { revision: number };

/**
 * Corner notifications: other players' public actions (from newly arrived log entries) and the
 * viewer's own auto-resolved single-choice decisions. Entries present when the game loads, and
 * entries that merely reappear after an undo or redo, never toast.
 */
export function useCornerToasts({
  events,
  players,
  viewerSeat,
  pendingChoice,
  autoResolved,
  historyGeneration,
  localNotes,
  board,
  explorations,
  breakthroughs,
  agendas,
  relics,
  table,
  ready,
}: CornerToastsInput) {
  const { muted } = useToastMute();
  const display = usePlayerIdentity();
  const displayRef = useRef(display);
  displayRef.current = display;
  const { enabled: recapOn } = useTurnRecapSetting();
  const recapRef = useRef(recapOn);
  recapRef.current = recapOn;
  const openTurn = useRef<OpenTurn | null>(null);
  const [recaps, setRecaps] = useState<TurnRecap[]>([]);
  const auto = useAutoResolveToasts();
  const [actions, setActions] = useState<Shown[]>([]);
  const [invasionToasts, setInvasionToasts] = useState<InvasionToast[]>([]);
  const [agendaToastList, setAgendaToasts] = useState<AgendaToast[]>([]);
  const [relicToastList, setRelicToasts] = useState<RelicToast[]>([]);
  const [rollToastList, setRollToasts] = useState<RollToast[]>([]);
  const [notices, setNotices] = useState<{ id: string; text: string; tag: string }[]>([]);
  const tableRef = useRef(table);
  tableRef.current = table;
  const pendingRef = useRef(pendingChoice);
  pendingRef.current = pendingChoice;
  const invasionTracker = useRef<InvasionTracker | null>(null);
  // The first board a client sees (load, undo) shows history, not news.
  const invasionQuiet = useRef(true);
  const seen = useRef<Set<string> | null>(null);
  const lastVp = useRef<Map<string, number> | null>(null);
  const lastAutoNonce = useRef<string | null>(null);
  const shownNotes = useRef<Set<string>>(new Set());
  const mutedRef = useRef(muted);
  mutedRef.current = muted;

  const push = useCallback((incoming: ActionToast[]) => {
    if (incoming.length === 0) return;
    setActions((prev) => {
      let list: ActionToast[] = prev;
      for (const toast of incoming) list = mergeActionToast(list, toast);
      return list.map((t) => {
        const before = prev.find((p) => p.id === t.id);
        const grew = before && before.parts.length !== t.parts.length;
        return { ...t, revision: before ? before.revision + (grew ? 1 : 0) : 0 };
      });
    });
  }, []);

  // One recap per call, and only the latest when several turns closed at once (a reconnect that
  // catches up on missed entries must not replay the game).
  const emitRecap = useCallback((closed: readonly OpenTurn[]) => {
    if (!recapRef.current || mutedRef.current) return;
    const recap = closed.length ? buildTurnRecap(closed[closed.length - 1]) : null;
    if (!recap) return;
    setRecaps((prev) => [...prev.filter((r) => r.id !== recap.id), recap].slice(-MAX_RECAPS));
  }, []);

  // A new history generation replaced the timeline: nothing open belongs to it.
  const lastGeneration = useRef(historyGeneration);
  useEffect(() => {
    if (lastGeneration.current !== historyGeneration) {
      lastGeneration.current = historyGeneration;
      openTurn.current = null;
      invasionTracker.current = null;
      invasionQuiet.current = true;
    }
  }, [historyGeneration]);

  // Bombardment, Space Cannon Defense and the whole invasion, from the board's last invasion step.
  useEffect(() => {
    if (!ready || !board) return;
    const quiet = invasionQuiet.current;
    invasionQuiet.current = false;
    const result = trackInvasion(invasionTracker.current, board, quiet, (seat) => displayRef.current(seat).label);
    invasionTracker.current = result.tracker;
    if (result.toasts.length === 0 || mutedRef.current) return;
    setInvasionToasts((prev) => [
      ...prev.filter((t) => !result.toasts.some((n) => n.id === t.id)),
      ...result.toasts,
    ].slice(-MAX_INVASION_TOASTS));
  }, [board, ready]);

  useEffect(() => {
    if (!ready) return;
    if (seen.current === null) {
      seen.current = new Set(events.map((e) => e.id));
      return;
    }
    // Undo removed entries of the open turn: forget it.
    if (openTurn.current) {
      const present = new Set(events.map((e) => e.id));
      if (openTurn.current.entries.some((e) => !present.has(e.id))) openTurn.current = null;
    }
    const fresh = events.filter((e) => !seen.current!.has(e.id));
    if (fresh.length === 0) return;
    for (const e of fresh) seen.current.add(e.id);
    const turns = trackTurns(openTurn.current, fresh, viewerSeat);
    openTurn.current = turns.open;
    emitRecap(turns.closed);
    if (mutedRef.current) return;
    // Dice are rolled in the open: everyone is told, the roller included.
    const rolled = rollToasts(fresh, (seat) => displayRef.current(seat).label, board);
    if (rolled.length > 0)
      setRollToasts((prev) => mergeRollToasts(prev, rolled, MAX_ROLL_TOASTS, board));
    // With the recap on, the acting player's own steps are summed up at the end of their turn.
    // Card plays and scoring always toast at once: the recap only names the acting player's own,
    // so another seat's Sabotage inside the turn would otherwise vanish.
    const live = recapRef.current
      ? fresh.filter((e) => !turns.tracked.has(e.id) || isCardOrScore(e))
      : fresh;
    push(live.flatMap((e) => actionToastFromEvent(e, viewerSeat, board) ?? []));
    // The viewer's own Imperial with nothing scoreable: the engine asks nothing, so say why.
    const imperial = ownImperialPlay(fresh, viewerSeat);
    if (imperial && viewerSeat) {
      const text = imperialNoScoreNotice({
        viewerSeat,
        table: tableRef.current,
        board,
        pendingChoice: pendingRef.current,
        label: (seat) => displayRef.current(seat).label,
      });
      if (text) setNotices((prev) => [...prev.filter((n) => n.id !== imperial.id), { id: imperial.id, text, tag: "Imperial" }].slice(-2));
    }
  }, [events, ready, viewerSeat, push, emitRecap, board]);

  // The viewer's own action menu means the previous player's turn is over.
  useEffect(() => {
    const open = openTurn.current;
    if (!open || !viewerSeat || !pendingChoice) return;
    if (pendingChoice.actor !== viewerSeat || !isTurnMenuChoice(pendingChoice)) return;
    openTurn.current = null;
    emitRecap([open]);
  }, [pendingChoice, viewerSeat, emitRecap]);

  useEffect(() => {
    if (!ready || !players) return;
    const now = new Map(players.map((p) => [p.id, p.victory_points]));
    const before = lastVp.current;
    lastVp.current = now;
    if (!before || mutedRef.current) return;
    push(
      victoryPointToasts(
        [...before].map(([id, victory_points]) => ({ id, victory_points })),
        players,
        viewerSeat,
      ),
    );
  }, [players, ready, viewerSeat, push]);

  // Exploration results come from the state's log (they are not decisions, so the event log has
  // no entry for them). What is there at load, or reappears after undo/redo, is history.
  const exploredCursor = useRef<number | null>(null);
  const exploredGeneration = useRef(historyGeneration);
  useEffect(() => {
    if (!ready || !explorations) return;
    if (exploredCursor.current === null || exploredGeneration.current !== historyGeneration) {
      exploredGeneration.current = historyGeneration;
      exploredCursor.current = explorations.length;
      return;
    }
    const { toasts, next } = explorationToasts(explorations, exploredCursor.current);
    exploredCursor.current = next;
    if (!mutedRef.current) push(toasts);
  }, [explorations, ready, historyGeneration, push]);

  // A breakthrough is earned from an expedition slice; it is not in the event log. What seats held
  // at load, or after undo/redo, is history.
  const lastBreakthroughs = useRef<Readonly<Record<string, string | null>> | null>(null);
  const breakthroughGeneration = useRef(historyGeneration);
  useEffect(() => {
    if (!ready || !breakthroughs) return;
    const before =
      breakthroughGeneration.current === historyGeneration ? lastBreakthroughs.current : null;
    breakthroughGeneration.current = historyGeneration;
    lastBreakthroughs.current = breakthroughs;
    if (!mutedRef.current) push(breakthroughToasts(before ? { ...before } : null, { ...breakthroughs }));
  }, [breakthroughs, ready, historyGeneration, push]);

  // A commander or hero is unlocked by the state, not by a logged decision: diff the seats'
  // leaders. What seats had unlocked at load, or after undo/redo, is history.
  const lastLeaders = useRef<Record<string, Record<string, string>> | null>(null);
  const leaderGeneration = useRef(historyGeneration);
  useEffect(() => {
    if (!ready || !players) return;
    const now = readLeaderStatuses(players as readonly Pick<PlayerView, "id" | "leaders">[]);
    const before = leaderGeneration.current === historyGeneration ? lastLeaders.current : null;
    leaderGeneration.current = historyGeneration;
    lastLeaders.current = now;
    if (!mutedRef.current) push(leaderUnlockToasts(before, now));
  }, [players, ready, historyGeneration, push]);

  // Agenda results come from the state's agenda log, like explorations: what is there at load, or
  // reappears after undo/redo, is history. The voter is told too: the result is news to all.
  const agendaCursor = useRef<number | null>(null);
  const agendaGeneration = useRef(historyGeneration);
  useEffect(() => {
    if (!ready || !agendas) return;
    if (agendaCursor.current === null || agendaGeneration.current !== historyGeneration) {
      agendaGeneration.current = historyGeneration;
      agendaCursor.current = agendas.length;
      return;
    }
    const seatName = (id: string) => {
      const seat = displayRef.current(id);
      return seat.position === null ? null : seat.label;
    };
    const { toasts, next } = agendaToasts(agendas, agendaCursor.current, seatName);
    agendaCursor.current = next;
    if (toasts.length === 0 || mutedRef.current) return;
    setAgendaToasts((prev) =>
      [...prev.filter((t) => !toasts.some((n) => n.id === t.id)), ...toasts].slice(-MAX_AGENDA_TOASTS),
    );
  }, [agendas, ready, historyGeneration]);

  // Relic gains come from the state's relic log, like agenda results: what is there at load, or
  // reappears after undo/redo, is history. Every seat is told, the gainer included.
  const relicCursor = useRef<number | null>(null);
  const relicGeneration = useRef(historyGeneration);
  useEffect(() => {
    if (!ready || !relics) return;
    if (relicCursor.current === null || relicGeneration.current !== historyGeneration) {
      relicGeneration.current = historyGeneration;
      relicCursor.current = relics.length;
      return;
    }
    const { toasts, next } = relicToasts(relics, relicCursor.current, viewerSeat);
    relicCursor.current = next;
    if (toasts.length === 0 || mutedRef.current) return;
    setRelicToasts((prev) =>
      [...prev.filter((t) => !toasts.some((n) => n.id === t.id)), ...toasts].slice(-MAX_RELIC_TOASTS),
    );
  }, [relics, ready, historyGeneration, viewerSeat]);

  const { showToast } = auto;
  useEffect(() => {
    const auto1 = autoResolvedToast(pendingChoice, viewerSeat);
    if (!auto1 || !pendingChoice || lastAutoNonce.current === pendingChoice.nonce) return;
    lastAutoNonce.current = pendingChoice.nonce;
    showToast(auto1.decisionType, auto1.selectedValue);
  }, [pendingChoice, viewerSeat, showToast]);

  useEffect(() => {
    if (!autoResolved) return;
    for (const note of autoResolved) {
      if (shownNotes.current.has(note.id)) continue;
      shownNotes.current.add(note.id);
      const times = note.count && note.count > 1 ? ` \u00d7${note.count}` : "";
      showToast(note.prompt, `${note.selected}${times}`, note.reason || undefined);
    }
  }, [autoResolved, showToast]);

  useEffect(() => {
    if (!localNotes) return;
    for (const note of localNotes) {
      if (shownNotes.current.has(note.id)) continue;
      shownNotes.current.add(note.id);
      showToast(note.prompt ?? "action phase", note.text);
    }
  }, [localNotes, showToast]);

  // Muting also clears what is on screen.
  useEffect(() => {
    if (muted) {
      setActions([]);
      setInvasionToasts([]);
      setAgendaToasts([]);
      setRelicToasts([]);
      setRollToasts([]);
      setNotices([]);
    }
  }, [muted]);
  useEffect(() => {
    if (muted || !recapOn) setRecaps([]);
  }, [muted, recapOn]);

  const dismissAction = useCallback(
    (id: string) => setActions((prev) => prev.filter((t) => t.id !== id)),
    [],
  );
  const { dismissToast } = auto;
  const dismiss = useCallback(
    (id: string) => {
      setRecaps((prev) => prev.filter((r) => r.id !== id));
      setInvasionToasts((prev) => prev.filter((t) => t.id !== id));
      setAgendaToasts((prev) => prev.filter((t) => t.id !== id));
      setRelicToasts((prev) => prev.filter((t) => t.id !== id));
      setRollToasts((prev) => prev.filter((t) => t.id !== id));
      setNotices((prev) => prev.filter((t) => t.id !== id));
      dismissAction(id);
      dismissToast(id);
    },
    [dismissAction, dismissToast],
  );

  const notifications = useMemo<AutoResolveNotification[]>(
    () => [
      ...auto.toasts,
      ...recaps.map(
        (r): AutoResolveNotification => ({
          id: r.id,
          kind: "recap",
          decisionType: "",
          selectedValue: "",
          actor: r.actor,
          text: r.text,
        }),
      ),
      ...invasionToasts.map(
        (t): AutoResolveNotification => ({
          id: t.id,
          kind: "invasion",
          decisionType: "",
          selectedValue: "",
          actor: t.actor,
          text: t.text,
          tag: INVASION_TAGS[t.kind],
        }),
      ),
      ...agendaToastList.map(
        (t): AutoResolveNotification => ({
          id: t.id,
          kind: "agenda",
          decisionType: "",
          selectedValue: "",
          text: t.text,
          tag: "Agenda",
        }),
      ),
      ...relicToastList.map(
        (t): AutoResolveNotification => ({
          id: t.id,
          kind: "relic",
          decisionType: "",
          selectedValue: "",
          actor: t.actor,
          text: t.text,
          tag: "Relic",
          emphasis: t.own,
          durationMs: RELIC_LINGER_MS,
        }),
      ),
      ...rollToastList.map(
        (t): AutoResolveNotification => ({
          id: t.id,
          kind: "notice",
          decisionType: "",
          selectedValue: "",
          text: t.text,
          tag: "Dice",
        }),
      ),
      ...notices.map(
        (t): AutoResolveNotification => ({
          id: t.id,
          kind: "notice",
          decisionType: "",
          selectedValue: "",
          text: t.text,
          tag: t.tag,
        }),
      ),
      ...actions.map(
        (t): AutoResolveNotification => ({
          id: t.id,
          kind: "action",
          decisionType: "",
          selectedValue: "",
          actor: t.actor,
          text: actionToastText(t),
          revision: t.revision,
          durationMs: LINGERING_CATEGORIES.has(t.category) ? LINGER_MS : undefined,
        }),
      ),
    ],
    [auto.toasts, actions, recaps, invasionToasts, agendaToastList, relicToastList, rollToastList, notices],
  );

  // Everything that was shown, kept so a toast that faded can be read again.
  const [history, setHistory] = useState<ToastHistoryEntry[]>([]);
  useEffect(() => {
    setHistory((prev) => recordToasts(prev, notifications));
  }, [notifications]);

  return { notifications, dismiss, muted, history };
}
