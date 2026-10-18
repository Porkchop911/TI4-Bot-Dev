import { AgendaPeekCard } from "./components/AgendaPeekCard.tsx";
import { describeAgendaPeek } from "./presentation/agendaPeek.ts";
import React, { useCallback, useEffect, useLayoutEffect, useMemo, useRef, useState } from "react";
import { ViewerRole } from "./protocol/types.ts";
import { useGameSession } from "./hooks/useGameSession.ts";
import { useLobbySession } from "./hooks/useLobbySession.ts";
import { shouldPlayTurnSound, useTurnSound } from "./hooks/useTurnSound.ts";
import { useTurnRedo } from "./hooks/useTurnRedo.ts";
import { playerDisplay } from "./presentation/playerDisplay.ts";
import { Board } from "./components/Board.tsx";
import { TurnStatusBar, TurnStatusDetails } from "./components/TurnStatusBar.tsx";
import { useBuildInfo } from "./buildInfo.ts";
import { engineLabel, useLoadedEngine } from "./offline/engineInfo.ts";
import { summarizeVersions } from "./versionInfo.ts";
import { VersionInfoDialog } from "./components/VersionInfo.tsx";
import { PasswordScreen } from "./access/PasswordScreen.tsx";
import { probeAccess, useAccessRequired } from "./access/accessGate.ts";
import { FeedbackLauncher, openFeedback } from "./feedback/FeedbackLauncher.tsx";
import { useRegisterFeedbackSource } from "./feedback/source.ts";
import { MobileTopMenu, menuAttention } from "./components/MobileTopMenu.tsx";
import { MobileMenuSlotContext } from "./presentation/MobileMenuSlot.tsx";
import { useCompactLayout } from "./hooks/useCompactLayout.ts";
import { PlayerSheet } from "./components/PlayerSheet.tsx";
import { rememberMyGame } from "./protocol/myGames.ts";
import { CreateLobby, LobbyStatus } from "./components/Lobby.tsx";
import { CatalogProvider } from "./components/CatalogProvider.tsx";
import { OfflineRoutes, isOfflinePath } from "./offline/OfflineRoutes.tsx";
import { useOfflineLauncher } from "./offline/useOfflineLauncher.ts";
import { registerLocalLauncher } from "./offline/local/register.ts";
import { useCapability } from "./offline/capabilities.ts";
import type { GameTransport } from "./protocol/transport.ts";
import { GameShell } from "./components/GameShell.tsx";
import { OddsEngineProvider } from "./components/OddsEngine.tsx";
import { usePresence } from "./hooks/usePresence.ts";
import { logHistoryKey as logHistoryKeyFor } from "./components/eventLogScroll.ts";
import { PlayerIdentityProvider } from "./presentation/PlayerIdentity.tsx";
import {
  advanceAgendaMemory,
  agendaInfoFromChoice,
  agendaPillLabel,
  deriveAgendaStrip,
  EMPTY_AGENDA_MEMORY,
  isAgendaDecision,
  type AgendaMemory,
} from "./presentation/agendaStrip.ts";
import { SeatInfoProvider } from "./presentation/SeatInfoContext.tsx";
import { PublicStateProvider } from "./presentation/PublicStateContext.tsx";
import { DecisionTableProvider } from "./components/PoliticsDecisionParts.tsx";
import { participantText } from "./presentation/participantText.ts";
import { CardDetails, CardSubject } from "./components/CardDetails.tsx";
import { TechnologyModal } from "./components/TechnologyModal.tsx";
import { ObjectivesModal } from "./components/ObjectivesModal.tsx";
import { resolveMapTargetSelection } from "./presentation/planetSelection.ts";
import { isPlanetSelectionChoice } from "./presentation/choiceModel.ts";
import {
  derivePaymentOffer,
  isPaymentChoice,
  paymentOptionForPlanet,
  paymentPlanetKey,
} from "./presentation/paymentDraft.ts";
import { UndoConfirmDialog } from "./components/UndoConfirmDialog.tsx";
import { PlanetBoostContext, PlanetInfluenceContext, PlanetResourcesContext, planetBoostMap, planetInfluenceMap, planetResourceMap } from "./presentation/PlanetResourcesContext.tsx";
import { useSecondaryPrepare, useTokenPrefill, useReadyPrefill } from "./hooks/useSecondaryPrepare.ts";
import { SecondaryPrepHost } from "./components/SecondaryPrepHost.tsx";
import { BluffHoldBar, BluffProvider } from "./components/BluffSelector.tsx";
import { PreparedHintProvider } from "./presentation/PreparedHint.tsx";
import { GONE_MESSAGE, UNREACHABLE_MESSAGE } from "./protocol/resilience.ts";
import { ConnectionProblem, ReconnectingChip, useAfterDelay } from "./components/ConnectionNotice.tsx";
import { readBreakthroughs } from "./presentation/breakthroughs.ts";
import { readExplorationLog } from "./presentation/explorationToasts.ts";
import { readAgendaLog } from "./presentation/agendaToasts.ts";
import { readRelicLog } from "./presentation/relicToasts.ts";
import { useBackUndo } from "./hooks/useBackUndo.ts";
import { CornerToastLayer } from "./components/CornerToastLayer.tsx";
import { NotificationLogButton, NotificationLogProvider } from "./components/NotificationLog.tsx";
import { PaymentDraftProvider, usePaymentDraftState } from "./presentation/PaymentDraftContext.tsx";
import {
  CommandTokenDraftProvider,
  useCommandTokenDraft,
} from "./presentation/CommandTokenDraftContext.tsx";
import { ReadyPlanetsDraftProvider, useReadyPlanetsDraft } from "./presentation/ReadyPlanetsDraftContext.tsx";

const DevDecisionGallery = import.meta.env.DEV
  ? React.lazy(() =>
      import("./dev/DecisionGallery.tsx").then(({ DecisionGallery }) => ({
        default: DecisionGallery,
      })),
    )
  : null;

const DevScenarioLauncher = import.meta.env.DEV
  ? React.lazy(() =>
      import("./dev/ScenarioLauncher.tsx").then(({ ScenarioLauncher }) => ({
        default: ScenarioLauncher,
      })),
    )
  : null;

const DevMapPickerGallery = import.meta.env.DEV
  ? React.lazy(() =>
      import("./dev/MapPickerGallery.tsx").then(({ MapPickerGallery }) => ({
        default: MapPickerGallery,
      })),
    )
  : null;
const DevToastGallery = import.meta.env.DEV
  ? React.lazy(() =>
      import("./dev/ToastGallery.tsx").then(({ ToastGallery }) => ({
        default: ToastGallery,
      })),
    )
  : null;

const OfflineGameScreen = React.lazy(() =>
  import("./offline/local/OfflineGameScreen.tsx").then((m) => ({ default: m.OfflineGameScreen })),
);

// The real offline launcher, when this build has the engine file (otherwise "Play offline" is absent).
registerLocalLauncher();
// Dev builds can register the in-memory offline launcher (screenshots, local work).
if (import.meta.env.DEV) void import("./offline/devMock.ts");

const storageKey = (gameId: string) => `ti4.player-session:${gameId}`;
const pathGameId = () =>
  /^\/games\/([^/]+)$/.exec(window.location.pathname)?.[1]
    ? decodeURIComponent(/^\/games\/([^/]+)$/.exec(window.location.pathname)![1])
    : null;

/**
 * The app, behind the playtest password screen and with the Feedback button on top. It renders once
 * the printed cards are loaded (`CatalogProvider`).
 */
export const App: React.FC = () => {
  const accessRequired = useAccessRequired();
  useEffect(() => {
    void probeAccess();
  }, []);
  if (accessRequired) return <PasswordScreen />;
  return (
    <>
      <CatalogProvider offline={isOfflinePath(window.location.pathname)}>
        <AppRoutes />
      </CatalogProvider>
      <FeedbackLauncher />
    </>
  );
};

const AppRoutes: React.FC = () => {
  const [gameId, setGameId] = useState(pathGameId);
  const [offline, setOffline] = useState(() => isOfflinePath(window.location.pathname));
  const offlineLauncher = useOfflineLauncher();
  const [error, setError] = useState<string | null>(null);
  const [token, setToken] = useState(() =>
    gameId ? (sessionStorage.getItem(storageKey(gameId)) ?? undefined) : undefined,
  );
  const navigate = (id: string | null, nextToken?: string) => {
    if (id) {
      if (nextToken) sessionStorage.setItem(storageKey(id), nextToken);
      history.pushState({}, "", `/games/${encodeURIComponent(id)}`);
    } else history.pushState({}, "", "/");
    setGameId(id);
    setToken(nextToken);
  };
  useLayoutEffect(() => {
    const receive = () => {
      const id = pathGameId();
      setOffline(isOfflinePath(window.location.pathname));
      setGameId(id);
      setToken(id ? (sessionStorage.getItem(storageKey(id)) ?? undefined) : undefined);
    };
    window.addEventListener("popstate", receive);
    return () => window.removeEventListener("popstate", receive);
  }, []);
  if (DevDecisionGallery && window.location.pathname === "/dev/decisions")
    return (
      <React.Suspense fallback={<main>Loading decision gallery…</main>}>
        <DevDecisionGallery />
      </React.Suspense>
    );
  if (DevMapPickerGallery && window.location.pathname === "/dev/map-picker")
    return (
      <React.Suspense fallback={<main>Loading map picker…</main>}>
        <DevMapPickerGallery />
      </React.Suspense>
    );
  if (DevToastGallery && window.location.pathname === "/dev/toasts")
    return (
      <React.Suspense fallback={<main>Loading toasts…</main>}>
        <DevToastGallery />
      </React.Suspense>
    );
  if (DevScenarioLauncher && window.location.pathname === "/dev/scenarios")
    return (
      <React.Suspense fallback={<main>Loading dev scenarios…</main>}>
        <DevScenarioLauncher />
      </React.Suspense>
    );
  if (offline)
    return (
      <OfflineRoutes
        onExit={() => {
          history.pushState({}, "", "/");
          setOffline(false);
        }}
        renderGame={(handle, banner) => (
          <React.Suspense fallback={<main className="lobby-page">Loading the game…</main>}>
            <OfflineGameScreen
              handle={handle}
              banner={banner}
              renderGame={(props) => <GameViewContainer {...props} />}
            />
          </React.Suspense>
        )}
      />
    );
  if (!gameId)
    return (
      <>
        <CreateLobby
          onPlayOffline={
            offlineLauncher
              ? () => {
                  history.pushState({}, "", "/offline");
                  setOffline(true);
                }
              : undefined
          }
          onError={setError}
          onCreated={(created) => navigate(created.game_id, created.player_session)}
          onOpenGame={(id, openToken) => navigate(id, openToken)}
        />
        {error && (
          <div className="session-error" role="alert">
            {error}
          </div>
        )}
      </>
    );
  return (
    <GameRoute
      key={gameId}
      gameId={gameId}
      token={token}
      onCredential={(credential) => {
        sessionStorage.setItem(storageKey(gameId), credential);
        rememberMyGame({ gameId, token: credential });
        setToken(credential);
      }}
      onCredentialInvalid={() => {
        sessionStorage.removeItem(storageKey(gameId));
        setToken(undefined);
      }}
      onForget={() => {
        sessionStorage.removeItem(storageKey(gameId));
        navigate(null);
      }}
      onHome={() => navigate(null)}
    />
  );
};

const GameRoute: React.FC<{
  gameId: string;
  token?: string;
  onCredential: (credential: string) => void;
  onCredentialInvalid: () => void;
  onForget: () => void;
  /** Back to the start screen without leaving the seat (it stays in My games). */
  onHome?: () => void;
}> = ({ gameId, token, onCredential, onCredentialInvalid, onForget, onHome }) => {
  const {
    lobby,
    playerId,
    error,
    connectionLost,
    gone,
    retry,
    dismissError,
    loading,
    invalidCredential,
    pendingAction,
    start,
    chooseMap,
    join,
    leave,
    removeBot,
    addRandomBots,
    setSeatFaction,
  } = useLobbySession(gameId, token);
  const [watching, setWatching] = useState(false);
  const invalidate = useCallback(() => onCredentialInvalid(), [onCredentialInvalid]);
  usePresence(gameId, token, invalidate);
  useRegisterFeedbackSource("lobby", () => ({
    gameId,
    viewer: token && playerId ? { role: "player", seat: playerId, playerSession: token } : { role: "spectator" },
    lobby,
  }));
  useEffect(() => {
    if (invalidCredential) onCredentialInvalid();
  }, [invalidCredential, onCredentialInvalid]);
  if (!lobby)
    return (
      <main className="lobby-page">
        <div className="panel lobby-panel">
          {loading && !gone ? "Loading lobby..." : null}
        </div>
        {gone ? (
          <ConnectionProblem kind="gone" message={GONE_MESSAGE} onRetry={retry} />
        ) : !loading ? (
          <ConnectionProblem kind="unreachable" message={UNREACHABLE_MESSAGE} onRetry={retry} />
        ) : null}
        {onHome && (gone || !loading) && (
          <button
            type="button"
            className="button button--secondary"
            data-testid="lobby-back-to-start"
            onClick={onHome}
          >
            Back to start
          </button>
        )}
      </main>
    );
  const viewer: ViewerRole =
    token && playerId
      ? { role: "player", seat: playerId, playerSession: token }
      : { role: "spectator" };
  const enter = async (nickname: string, id?: string) => {
    const credential = await join(nickname, id);
    if (credential) {
      setWatching(false);
      onCredential(credential);
    }
  };
  const leaveLobby = async () => {
    if (await leave()) onForget();
  };
  return (
    <>
      {error && (
        <div className="session-error session-error--dismissable" role="alert" data-testid="lobby-error">
          <span>{participantText(error, lobby, [])}</span>
          <button
            type="button"
            className="button button--secondary button--sm"
            data-testid="lobby-error-dismiss"
            onClick={dismissError}
          >
            Dismiss
          </button>
        </div>
      )}
      {lobby.phase === "running" && (playerId || watching) ? null : (
        <ReconnectingChip visible={connectionLost} />
      )}
      {lobby.phase === "running" && (playerId || watching) ? (
        <GameViewContainer
          lobbyConnectionLost={connectionLost}
          key={`${gameId}:${token ?? "watch"}`}
          gameId={gameId}
          lobby={lobby}
          viewer={viewer}
          onLeave={onForget}
        />
      ) : (
        <LobbyStatus
          lobby={lobby}
          playerId={playerId}
          watching={watching}
          pendingAction={pendingAction}
          onStart={() => void start()}
          onLeave={() => void leaveLobby()}
          onJoin={(name) => void enter(name)}
          onTakeover={(id, name) => void enter(name, id)}
          onChooseMap={chooseMap}
          onWatch={() => setWatching(true)}
          onRemoveBot={(targetId) => removeBot(targetId)}
          onAddRandomBots={(options) => addRandomBots(options)}
          onSetSeatFaction={(targetId, faction) => setSeatFaction(targetId, faction)}
          onHome={onHome}
        />
      )}
    </>
  );
};

/** Gives a seated player the bluff selector on the player mat; a spectator gets none. */
const BluffScope: React.FC<{
  gameId: string;
  seat: string | undefined;
  intent: import("./protocol/types.ts").ReactionIntentStateMsg | null;
  round: number;
  onChange: (triggers: string[]) => void;
  children: React.ReactNode;
}> = ({ gameId, seat, intent, round, onChange, children }) =>
  seat ? (
    <BluffProvider gameId={gameId} seat={seat} intent={intent} round={round} onChange={onChange}>
      {children}
    </BluffProvider>
  ) : (
    <>{children}</>
  );

const GameViewContainer: React.FC<{
  lobbyConnectionLost?: boolean;
  gameId: string;
  lobby: import("./protocol/types.ts").LobbyDto;
  viewer: ViewerRole;
  onLeave?: () => void;
  /** An offline game passes its local transport; online games use the server client. */
  createTransport?: () => GameTransport;
}> = ({ gameId, lobby, viewer, lobbyConnectionLost, createTransport }) => {
  const {
    capabilities,
    status,
    gameVersion,
    snapshot,
    pendingChoice: realPendingChoice,
    turnStatus,
    lastError,
    fatal,
    reconnecting: reconnectingState,
    retryConnection,
    events,
    history: gameHistory,
    submitChoice,
    setReactionMode,
    reactionIntent,
    setReactionIntent,
    passReactionHold,
    changeHistory,
    fetchReplay,
    fetchTurnRedoStatus,
    turnRedoCommand,
    submitMovementBatch,
    submitBatch,
    previewSecondary,
    queryReach,
    queryEngine,
    gameVersions,
    batchResume,
    resumeBatch,
    dismissBatchResume,
  } = useGameSession({ gameId, viewer, createTransport });
  const buildInfo = useBuildInfo();
  // What this session can do: the transport says what it offers, the offline shell's context what
  // the product offers. Features that are off are hidden, not disabled.
  const shellTurnRedo = useCapability("turnRedo");
  const shellMultiplayer = useCapability("multiplayer");
  const turnRedoOn = capabilities.turnRedo && shellTurnRedo;
  const multiplayerOn = capabilities.multiplayer && shellMultiplayer;
  const loadedEngine = useLoadedEngine();
  const versionSummary = summarizeVersions({
    hosted: gameVersions ? gameVersions() : null,
    state: snapshot?.state,
    serverCommit: buildInfo.commit,
    engine: engineLabel(loadedEngine),
  });
  const [isVersionsOpen, setIsVersionsOpen] = useState(false);
  useRegisterFeedbackSource("game", () => ({
    gameId,
    viewer,
    snapshot,
    pendingChoice: realPendingChoice,
    lobby,
    events,
    versions: versionSummary,
  }));
  const reconnecting = useAfterDelay(
    (status !== "connected" || Boolean(reconnectingState) || Boolean(lobbyConnectionLost)) && !fatal,
    1_200,
  );
  const { playTurnNotification } = useTurnSound();
  // The event log drops the reader's manual expansion only for a new history generation (undo,
  // redo, restore). A reconnect re-sends the same generation with a fresh events array and keeps
  // the reader's place; servers without a generation fall back to the array identity.
  const logHistoryKey = useRef<unknown>(null);
  const logEventsIdentity = useRef<unknown>(null);
  if (
    snapshot?.type === "initial_snapshot" &&
    snapshot.events &&
    logEventsIdentity.current !== snapshot.events
  )
    logEventsIdentity.current = snapshot.events;
  logHistoryKey.current = logHistoryKeyFor(gameHistory?.generation, logEventsIdentity.current);
  const [localNotes, setLocalNotes] = useState<{ id: string; text: string; prompt?: string }[]>([]);
  const [historyBusy, setHistoryBusy] = useState(false);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const [undoRequest, setUndoRequest] = useState<{
    action: import("./protocol/client.ts").HistoryChange;
    steps: number;
  } | null>(null);
  const onChangeHistory = (action: import("./protocol/client.ts").HistoryChange, steps = 1) => {
    if (historyBusy) return;
    // Undo rewinds shared history, so ask once in a styled dialog before running it.
    if (action === "undo_pipeline" || steps > 1) {
      setUndoRequest({ action, steps });
      return;
    }
    void runHistoryChange(action);
  };
  /** Resolves true when the change went through (false: the error is on screen). */
  const runHistoryChange = (action: import("./protocol/client.ts").HistoryChange): Promise<boolean> => {
    setHistoryBusy(true);
    setHistoryError(null);
    return changeHistory(action)
      .then(() => {
        setSelectedOptionId(undefined);
        setSelectedPlanetId(null);
        setSelectedSystemId(null);
        setCardSubject(null);
        return true;
      })
      .catch((error: unknown) => {
        setHistoryError(error instanceof Error ? error.message : String(error));
        return false;
      })
      .finally(() => setHistoryBusy(false));
  };
  // Single-player undo: take back the human's last answer (the engine's live undo; the bots'
  // decisions after it are replayed, not undone one by one).
  const undoLastDecision = (): Promise<boolean> =>
    historyBusy ? Promise.resolve(false) : runHistoryChange("undo");
  const userSeat = viewer.role === "player" ? viewer.seat : undefined;
  const singlePlayerUndo = !multiplayerOn && Boolean(createTransport) && userSeat === lobby.host_player_id;
  const backNote = useBackUndo({
    enabled: singlePlayerUndo,
    canUndo: gameHistory.can_undo === true,
    busy: historyBusy,
    undo: undoLastDecision,
  });
  const turnRedo = useTurnRedo({
    enabled: viewer.role === "player" && turnRedoOn,
    fetchStatus: fetchTurnRedoStatus,
    command: turnRedoCommand,
    gameVersion,
    generation: gameHistory.generation ?? 0,
    viewerSeat: userSeat,
    onTimelineChanged: () => {
      setSelectedOptionId(undefined);
      setSelectedPlanetId(null);
      setSelectedSystemId(null);
      setCardSubject(null);
    },
  });
  // Strategy-card secondary preparation: while another seat resolves a card the viewer may prepare
  // an answer with the usual UI. `pendingChoice` is then a client-made stand-in for the question
  // (answered into the local plan, never submitted), or nothing while an auto-played answer is
  // being sent. The engine's own decision is `realPendingChoice`.
  const productionSubmitRef = useRef<
    | ((units: string[], destination: string, real: import("./protocol/types.ts").PendingChoiceDto) => Promise<void>)
    | null
  >(null);
  const prep = useSecondaryPrepare({
    gameId,
    viewerSeat: userSeat,
    players: snapshot?.view.players ?? [],
    events,
    board: snapshot?.view.board,
    phase: snapshot?.view.phase,
    history: gameHistory,
    realChoice: realPendingChoice,
    busy: historyBusy,
    submitChoice,
    submitBatch,
    previewSecondary: userSeat ? previewSecondary : undefined,
    refreshKey: `${events.length}:${events[events.length - 1]?.id ?? ""}:${gameHistory.generation ?? 0}`,
    submitProduction: (units, destination, real) =>
      productionSubmitRef.current
        ? productionSubmitRef.current(units, destination, real)
        : Promise.reject(new Error("The production builder is not available")),
  });
  const pendingChoice = prep.shownChoice;
  const paymentDraft = usePaymentDraftState(pendingChoice?.nonce);
  const explorationLog = useMemo(() => readExplorationLog(snapshot?.state), [snapshot?.state]);
  const breakthroughs = useMemo(() => readBreakthroughs(snapshot?.state), [snapshot?.state]);
  const agendaLog = useMemo(() => readAgendaLog(snapshot?.state), [snapshot?.state]);
  const relicLog = useMemo(() => readRelicLog(snapshot?.state), [snapshot?.state]);
  const planetResources = useMemo(() => planetResourceMap(snapshot?.view.board), [snapshot?.view.board]);
  const planetInfluence = useMemo(() => planetInfluenceMap(snapshot?.view.board), [snapshot?.view.board]);
  const planetBoosts = useMemo(() => planetBoostMap(snapshot?.view.board), [snapshot?.view.board]);
  const tokenDraft = useCommandTokenDraft(pendingChoice, planetResources);
  // Diplomacy: the planets picked for its ready question (bar, list and map share them).
  const readyDraft = useReadyPlanetsDraft(
    pendingChoice && pendingChoice.actor === userSeat ? pendingChoice : null,
    snapshot?.view.board,
  );
  const [selectedOptionId, setSelectedOptionId] = useState<string>();
  const [selectedPlanetId, setSelectedPlanetId] = useState<string | null>(null);
  const [selectedSystemId, setSelectedSystemId] = useState<string | null>(null);
  const [cardSubject, setCardSubject] = useState<CardSubject | null>(null);
  const [isTechModalOpen, setIsTechModalOpen] = useState(false);
  const [isObjectivesModalOpen, setIsObjectivesModalOpen] = useState(false);
  const closeObjectives = useCallback(() => setIsObjectivesModalOpen(false), []);
  // Phones: one slim top bar and a menu sheet; the board portals its controls into the sheet.
  const compactLayout = useCompactLayout();
  const [menuSlotElement, setMenuSlotElement] = useState<HTMLElement | null>(null);
  const menuSlotValue = useMemo(
    () => ({ slot: menuSlotElement, setSlot: setMenuSlotElement }),
    [menuSlotElement],
  );

  // The agenda being decided stays on screen for the whole agenda phase: the strip under the top bar
  // and the minimized pill. The phase's memory carries the agenda through waiting states and
  // decisions that do not name it, and is dropped when the phase ends.
  const agendaMemoryRef = useRef<AgendaMemory>(EMPTY_AGENDA_MEMORY);
  const agendaPhase = snapshot?.view.phase;
  agendaMemoryRef.current = advanceAgendaMemory(agendaMemoryRef.current, {
    phase: agendaPhase,
    round: snapshot?.view.round,
    info: agendaInfoFromChoice(realPendingChoice),
    decision: isAgendaDecision(realPendingChoice),
  });
  const agendaStrip = snapshot
    ? deriveAgendaStrip({
        phase: agendaPhase,
        choice: realPendingChoice,
        memory: agendaMemoryRef.current,
        viewerSeat: userSeat,
        speaker: snapshot.view.speaker,
        label: (seat) => playerDisplay(lobby, snapshot.view.seating_order, seat).label,
        waitingForReactions: turnStatus?.kind === "waiting_for_reactions",
      })
    : null;
  const agendaPill = agendaStrip ? agendaPillLabel(agendaStrip, realPendingChoice) : null;

  // One sound when the decision passes to the viewer, not one per decision: a turn is a run of
  // questions for the same seat (menu, system, moves, production, ...) and only the first beeps.
  const wasViewerDecisionRef = useRef(false);
  useEffect(() => {
    const isViewerDecision = Boolean(
      realPendingChoice && userSeat && realPendingChoice.actor === userSeat,
    );
    if (shouldPlayTurnSound(wasViewerDecisionRef.current, isViewerDecision)) playTurnNotification();
    wasViewerDecisionRef.current = isViewerDecision;
  }, [realPendingChoice?.nonce, userSeat, realPendingChoice?.actor, playTurnNotification]);

  useEffect(() => {
    setSelectedOptionId(undefined);
    setSelectedPlanetId(null);
  }, [pendingChoice?.nonce]);
  // The usual UI shows the prepared answer already selected (the confirm stays the player's).
  const prefillId = prep.resolution.kind === "option" ? prep.resolution.optionId : null;
  const prefillNonce = prep.realChoice?.nonce;
  useEffect(() => {
    if (prefillId && prefillNonce && !prep.holding) setSelectedOptionId(prefillId);
  }, [prefillId, prefillNonce, pendingChoice?.nonce]); // eslint-disable-line react-hooks/exhaustive-deps
  // Leadership: the usual token panel opens with the prepared purchase already staged (tokens, their
  // pools and the payment on the map). Once per question, so the player's own changes stay.
  useTokenPrefill(prep, pendingChoice?.nonce, tokenDraft);
  // Diplomacy: the ready bar opens with the prepared planets already picked.
  useReadyPrefill(prep, pendingChoice?.nonce, readyDraft);
  const cardIsVisible =
    cardSubject &&
    snapshot &&
    (cardSubject.kind === "publicObjective"
      ? snapshot.view.table.revealed_objectives.includes(cardSubject.id)
      : cardSubject.kind === "strategy"
        ? snapshot.view.players.some((player) => player.strategy_cards.includes(cardSubject.id))
        : cardSubject.kind === "action"
          ? snapshot.view.players.some(
              (player) =>
                player.id === userSeat && player.held_action_cards?.includes(cardSubject.id),
            )
          : snapshot.view.players.some(
              (player) =>
                player.scored_secret_objectives?.includes(cardSubject.id) ||
                (player.id === userSeat && player.held_secret_objectives?.includes(cardSubject.id)),
            ));
  const handleSelectTarget = (systemId: string, planetId?: string) => {
    if (!pendingChoice || pendingChoice.actor !== userSeat) return;
    // Diplomacy: a click on an exhausted planet picks or drops it (Confirm sends them together).
    if (readyDraft.view) {
      if (planetId) readyDraft.toggle(planetId);
      return;
    }
    // Leadership purchase: a click on a planet that can pay flips it in the staged payment.
    if (tokenDraft.mapPayment) {
      if (planetId && tokenDraft.mapPayment.offer.planets.some((p) => p.planetId === planetId)) {
        tokenDraft.togglePlanet(planetId);
      }
      return;
    }
    // Paying: a click on a payable planet stages or unstages it (shared with the payment list).
    if (isPaymentChoice(pendingChoice)) {
      if (!planetId) return;
      const option = paymentOptionForPlanet(derivePaymentOffer(pendingChoice), planetId);
      if (!option) return;
      const staged = paymentDraft.draft.planetIds.find((id) => paymentPlanetKey(id) === planetId);
      paymentDraft.togglePlanet(staged ?? option.id);
      return;
    }
    const selection = resolveMapTargetSelection(
      pendingChoice,
      systemId,
      planetId,
      snapshot?.view.board,
    );
    if (selection.kind === "ignore") return;
    setSelectedOptionId(selection.optionId);
    setSelectedPlanetId(selection.planetId);
  };
  return (
    <OddsEngineProvider local={queryEngine}>
    <PlayerIdentityProvider lobby={lobby} seatingOrder={snapshot?.view.seating_order ?? []}>
    <SeatInfoProvider players={snapshot?.view.players} viewerSeat={userSeat}>
    <PublicStateProvider state={snapshot?.state}>
    <DecisionTableProvider table={snapshot?.view ?? null}>
    <NotificationLogProvider inline>
    <BluffScope
      gameId={gameId}
      seat={multiplayerOn ? userSeat : undefined}
      intent={reactionIntent}
      round={snapshot?.view.round ?? 0}
      onChange={setReactionIntent}
    >
    <PreparedHintProvider
      value={
        !prep.preparing && prep.realChoice
          ? prep.resolution.kind === "option"
            ? { optionId: prep.resolution.optionId, text: prep.resolution.text }
            : prep.resolution.kind === "production"
              ? { optionId: "", text: prep.resolution.text, builds: prep.resolution.units }
              : null
          : null
      }
    >
      {/* The provider wraps the rest unindented to keep this diff small. */}
      <PaymentDraftProvider value={paymentDraft}>
      <PlanetResourcesContext.Provider value={planetResources}>
      <PlanetInfluenceContext.Provider value={planetInfluence}>
      <PlanetBoostContext.Provider value={planetBoosts}>
      <CommandTokenDraftProvider value={tokenDraft}>
      <ReadyPlanetsDraftProvider value={readyDraft}>
      {undoRequest && (
        <UndoConfirmDialog
          steps={undoRequest.steps}
          onCancel={() => setUndoRequest(null)}
          onConfirm={() => {
            const { action } = undoRequest;
            setUndoRequest(null);
            void runHistoryChange(action);
          }}
        />
      )}
      {historyError && (
        <div className="session-error session-error--dismissable" role="alert" data-testid="history-error">
          <span>{historyError}</span>
          <button
            type="button"
            className="button button--secondary button--sm"
            data-testid="history-error-dismiss"
            onClick={() => setHistoryError(null)}
          >
            Dismiss
          </button>
        </div>
      )}
      <ReconnectingChip visible={reconnecting} />
      <AgendaPeekCard peek={describeAgendaPeek(snapshot?.view.players ?? [], userSeat)} />
      {fatal && (
        <ConnectionProblem kind={fatal.kind} message={fatal.message} onRetry={retryConnection} />
      )}
      {backNote && (
        <div role="status" aria-live="polite" className="back-undo-note" data-testid="back-undo-note">
          {backNote}
        </div>
      )}
      <CornerToastLayer
        events={events}
        board={snapshot?.view.board}
        players={snapshot?.view.players}
        viewerSeat={userSeat}
        pendingChoice={realPendingChoice}
        historyGeneration={gameHistory.generation}
        autoResolved={snapshot?.type === "state_update" ? snapshot.auto_resolved : undefined}
        ready={Boolean(snapshot)}
        localNotes={localNotes}
        explorations={explorationLog}
        breakthroughs={breakthroughs}
        agendas={agendaLog}
        relics={relicLog}
        table={snapshot?.view.table}
      />
      <MobileMenuSlotContext.Provider value={compactLayout ? menuSlotValue : null}>
      <GameShell
        header={
          <div className={`game-header${compactLayout ? " game-header--compact" : ""}`}>
            <TurnStatusBar
              status={turnStatus}
              view={snapshot?.view ?? null}
              gameVersion={gameVersion}
              connectionStatus={status}
              userSeat={userSeat}
              compact={compactLayout}
              trailing={
                compactLayout ? (
                  <MobileTopMenu
                    onOpenFeedback={buildInfo.isPublic || createTransport ? openFeedback : undefined}
                    versions={versionSummary}
                    attention={menuAttention({
                      connected: status === "connected",
                      yourDecision: Boolean(
                        userSeat && realPendingChoice && realPendingChoice.actor === userSeat,
                      ),
                      yourTurn: Boolean(
                        userSeat &&
                          turnStatus &&
                          ((turnStatus.kind === "waiting_for_decision" && turnStatus.seat === userSeat) ||
                            (turnStatus.kind === "active_turn" && turnStatus.player === userSeat)),
                      ),
                    })}
                    status={
                      <TurnStatusDetails
                        status={turnStatus}
                        view={snapshot?.view ?? null}
                        gameVersion={gameVersion}
                        connectionStatus={status}
                        userSeat={userSeat}
                      />
                    }
                    onOpenTechnologies={() => setIsTechModalOpen(true)}
                    onOpenObjectives={() => setIsObjectivesModalOpen(true)}
                  />
                ) : undefined
              }
            />
            <BluffHoldBar holding={Boolean(reactionIntent?.holding)} onPass={passReactionHold} />
            {!compactLayout && (
            <div className="game-header__actions">
              <button
                type="button"
                data-testid="technology-modal-button"
                onClick={() => setIsTechModalOpen(true)}
                className="button button--secondary"
              >
                Technologies
              </button>
              <button
                type="button"
                data-testid="objectives-modal-button"
                onClick={() => setIsObjectivesModalOpen(true)}
                className="button button--secondary"
              >
                Objectives
              </button>
              <button
                type="button"
                data-testid="version-info-button"
                onClick={() => setIsVersionsOpen(true)}
                className="button button--secondary"
              >
                Versions
              </button>
              <NotificationLogButton />
            </div>
            )}
            {isVersionsOpen && <VersionInfoDialog summary={versionSummary} onClose={() => setIsVersionsOpen(false)} />}
          </div>
        }
        board={
          snapshot ? (
            <Board
              board={snapshot.view.board}
              seatingOrder={snapshot.view.seating_order}
              players={snapshot.view.players}
              pendingChoice={pendingChoice}
              viewerSeat={userSeat}
              selectedSystemId={selectedSystemId}
              queryReach={queryReach}
              onSelectSystem={(id) => {
                setSelectedSystemId(id);
                setCardSubject(null);
                // In a planet selection a hex click only inspects; it never drops the pick.
                if (
                  id &&
                  pendingChoice &&
                  pendingChoice.actor === userSeat &&
                  !isPlanetSelectionChoice(pendingChoice)
                ) {
                  const match = pendingChoice.options.find(
                    (option) =>
                      String(option.payload?.system ?? option.payload?.to ?? option.id) === id,
                  );
                  if (!match) {
                    setSelectedOptionId(undefined);
                  }
                }
              }}
              onSelectOptionId={(id) => {
                if (pendingChoice?.options.some((option) => option.id === id))
                  setSelectedOptionId(id);
              }}
              onSelectTarget={handleSelectTarget}
            />
          ) : (
            <div className="game-loading">Loading game state...</div>
          )
        }
        boardView={snapshot?.view.board}
        tacticalFacts={snapshot?.tactical}
        activeSystemId={
          typeof snapshot?.state.active_system === "string" ? snapshot.state.active_system : null
        }
        playerSheet={
          snapshot ? (
            <PlayerSheet
              players={snapshot.view.players}
              userSeat={userSeat}
              seatingOrder={snapshot.view.seating_order}
              revealedObjectives={snapshot.view.table.revealed_objectives}
              board={snapshot.view.board}
              table={snapshot.view.table}
              reactionModes={snapshot.reaction_modes}
              onSetReactionMode={userSeat ? setReactionMode : undefined}
              onInspectCard={(subject) => {
                setSelectedSystemId(null);
                setCardSubject(subject);
              }}
            />
          ) : null
        }
        onOpenObjectives={() => setIsObjectivesModalOpen(true)}
        objectivesOpen={isObjectivesModalOpen}
        onCloseObjectives={closeObjectives}
        detail={
          cardSubject &&
          cardIsVisible && (
            <CardDetails subject={cardSubject} onClose={() => setCardSubject(null)} />
          )
        }
        events={events}
        currentPath={snapshot?.current_path}
        logHistoryKey={logHistoryKey.current}
        history={gameHistory}
        historyBusy={historyBusy}
        onUndoLast={!multiplayerOn && userSeat === lobby.host_player_id ? undoLastDecision : undefined}
        newestFirst={!multiplayerOn}
        onAutoSubmitNotice={(note) => setLocalNotes((prev) => [...prev, note])}
        onChangeHistory={userSeat === lobby.host_player_id ? onChangeHistory : undefined}
        onFetchReplay={userSeat ? fetchReplay : undefined}
        turnRedo={
          userSeat && turnRedoOn
            ? {
                status: turnRedo.status,
                busy: turnRedo.busy,
                error: turnRedo.error,
                onRequest: turnRedo.request,
                onAutoplay: turnRedo.autoplay,
                onRestore: turnRedo.restore,
                onKeep: turnRedo.keep,
                viewerSeat: userSeat,
                local: !multiplayerOn,
                seats:
                  multiplayerOn && userSeat === lobby.host_player_id && snapshot
                    ? [userSeat, ...snapshot.view.seating_order.filter((id) => id !== userSeat)].map(
                        (id) => ({
                          id,
                          label: playerDisplay(lobby, snapshot.view.seating_order, id).label,
                        }),
                      )
                    : undefined,
              }
            : undefined
        }
        choice={pendingChoice}
        agenda={agendaStrip}
        activeEffects={snapshot?.view.active_effects}
        agendaPill={agendaPill}
        viewerSeat={userSeat}
        players={snapshot?.view.players}
        turn={
          snapshot
            ? {
                phase: snapshot.view.phase,
                activePlayer: snapshot.view.active_player ?? null,
                over: turnStatus?.kind === "game_over" || snapshot.view.finished === true,
                othersDeciding:
                  turnStatus?.kind === "waiting_for_reactions" ||
                  (turnStatus?.kind === "waiting_for_decision" && turnStatus.seat !== userSeat),
              }
            : undefined
        }
        revealedObjectives={snapshot?.view.table.revealed_objectives}
        scoredObjectives={snapshot?.view.table.scored_objectives}
        objectiveProgress={snapshot?.view.table.objective_progress}
        homeSystemLost={snapshot?.view.table.home_system_lost}
        onSubmitChoice={prep.preparing ? prep.prepareSubmit : submitChoice}
        prepOverlay={
          <SecondaryPrepHost prep={prep} busy={historyBusy} />
        }
        reactionModes={snapshot?.reaction_modes}
        onSetReactionMode={userSeat ? setReactionMode : undefined}
        onSubmitMovementBatch={submitMovementBatch}
        onSubmitBasketBatch={prep.preparing ? prep.prepareBatch : submitBatch}
        productionSubmitRef={productionSubmitRef}
        batchResume={batchResume}
        onResumeBatch={resumeBatch}
        onDismissBatchResume={dismissBatchResume}
        lastError={lastError}
        selectedOptionId={selectedOptionId}
        onSelectOption={setSelectedOptionId}
        selectedPlanetId={selectedPlanetId}
        onSelectPlanet={setSelectedPlanetId}
        onShowSystem={setSelectedSystemId}
      />
      </MobileMenuSlotContext.Provider>
      <TechnologyModal
        isOpen={isTechModalOpen}
        onClose={() => setIsTechModalOpen(false)}
        players={snapshot?.view.players}
        viewerSeat={userSeat}
      />
      <ObjectivesModal
        isOpen={
          isObjectivesModalOpen &&
          pendingChoice?.context?.subtype !== "score_objective" &&
          pendingChoice?.context?.subtype !== "imperial_score_objective"
        }
        onClose={() => setIsObjectivesModalOpen(false)}
        revealedObjectives={snapshot?.view.table.revealed_objectives}
        scoredObjectives={snapshot?.view.table.scored_objectives}
        objectiveProgress={snapshot?.view.table.objective_progress}
        homeSystemLost={snapshot?.view.table.home_system_lost}
        players={snapshot?.view.players}
        viewerSeat={userSeat}
        board={snapshot?.view.board}
        onInspectCard={(subject) => {
          setSelectedSystemId(null);
          setCardSubject(subject);
        }}
      />
      </ReadyPlanetsDraftProvider>
      </CommandTokenDraftProvider>
      </PlanetBoostContext.Provider>
      </PlanetInfluenceContext.Provider>
      </PlanetResourcesContext.Provider>
      </PaymentDraftProvider>
    </PreparedHintProvider>
    </BluffScope>
    </NotificationLogProvider>
    </DecisionTableProvider>
    </PublicStateProvider>
    </SeatInfoProvider>
    </PlayerIdentityProvider>
    </OddsEngineProvider>
  );
};
