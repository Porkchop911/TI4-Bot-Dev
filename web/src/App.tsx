import React, { useCallback, useEffect, useLayoutEffect, useRef, useState } from "react";
import { ViewerRole } from "./protocol/types.ts";
import { useGameSession } from "./hooks/useGameSession.ts";
import { useLobbySession } from "./hooks/useLobbySession.ts";
import { Board } from "./components/Board.tsx";
import { TurnStatusBar } from "./components/TurnStatusBar.tsx";
import { PlayerSheet } from "./components/PlayerSheet.tsx";
import { CreateLobby, LobbyStatus } from "./components/Lobby.tsx";
import { GameShell } from "./components/GameShell.tsx";
import { usePresence } from "./hooks/usePresence.ts";
import { PlayerIdentityProvider } from "./presentation/PlayerIdentity.tsx";
import { participantText } from "./presentation/participantText.ts";
import { CardDetails, CardSubject } from "./components/CardDetails.tsx";
import { TechnologyModal } from "./components/TechnologyModal.tsx";
import { ObjectivesModal } from "./components/ObjectivesModal.tsx";

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

const storageKey = (gameId: string) => `ti4.player-session:${gameId}`;
const pathGameId = () =>
  /^\/games\/([^/]+)$/.exec(window.location.pathname)?.[1]
    ? decodeURIComponent(/^\/games\/([^/]+)$/.exec(window.location.pathname)![1])
    : null;

export const App: React.FC = () => {
  const [gameId, setGameId] = useState(pathGameId);
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
  if (DevScenarioLauncher && window.location.pathname === "/dev/scenarios")
    return (
      <React.Suspense fallback={<main>Loading dev scenarios…</main>}>
        <DevScenarioLauncher />
      </React.Suspense>
    );
  if (!gameId)
    return (
      <>
        <CreateLobby
          onError={setError}
          onCreated={(created) => navigate(created.game_id, created.player_session)}
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
    />
  );
};

const GameRoute: React.FC<{
  gameId: string;
  token?: string;
  onCredential: (credential: string) => void;
  onCredentialInvalid: () => void;
  onForget: () => void;
}> = ({ gameId, token, onCredential, onCredentialInvalid, onForget }) => {
  const {
    lobby,
    playerId,
    error,
    loading,
    invalidCredential,
    pendingAction,
    setReady,
    start,
    reorder,
    join,
    leave,
    addBot,
    removeBot,
  } = useLobbySession(gameId, token);
  const [watching, setWatching] = useState(false);
  const invalidate = useCallback(() => onCredentialInvalid(), [onCredentialInvalid]);
  usePresence(gameId, token, invalidate);
  useEffect(() => {
    if (invalidCredential) onCredentialInvalid();
  }, [invalidCredential, onCredentialInvalid]);
  if (!lobby)
    return (
      <main className="lobby-page">
        <div className="panel lobby-panel">
          {loading ? "Loading lobby..." : "Unable to load lobby."}
        </div>
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
        <div className="session-error" role="alert">
          {participantText(error, lobby, [])} Check the lobby and try again.
        </div>
      )}
      {lobby.phase === "running" && (playerId || watching) ? (
        <GameViewContainer
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
          onReady={(ready) => void setReady(ready)}
          onStart={() => void start()}
          onLeave={() => void leaveLobby()}
          onJoin={(name) => void enter(name)}
          onTakeover={(id, name) => void enter(name, id)}
          onReorder={(ids) => void reorder(ids)}
          onWatch={() => setWatching(true)}
          onAddBot={(password, name) => addBot(password, name)}
          onRemoveBot={(targetId) => removeBot(targetId)}
        />
      )}
    </>
  );
};

const GameViewContainer: React.FC<{
  gameId: string;
  lobby: import("./protocol/types.ts").LobbyDto;
  viewer: ViewerRole;
  onLeave?: () => void;
}> = ({ gameId, lobby, viewer }) => {
  const {
    status,
    gameVersion,
    snapshot,
    pendingChoice,
    turnStatus,
    lastError,
    events,
    history: gameHistory,
    submitChoice,
    changeHistory,
    submitMovementBatch,
    submitBatch,
  } = useGameSession({ gameId, viewer });
  const logHistoryKey = useRef<unknown>(null);
  if (
    snapshot?.type === "initial_snapshot" &&
    snapshot.events &&
    logHistoryKey.current !== snapshot.events
  )
    logHistoryKey.current = snapshot.events;
  const [historyBusy, setHistoryBusy] = useState(false);
  const [historyError, setHistoryError] = useState<string | null>(null);
  const onChangeHistory = (action: import("./protocol/client.ts").HistoryChange, steps = 1) => {
    if (
      historyBusy ||
      (action === "undo_pipeline" &&
        !window.confirm("Undo the latest action and its follow-up decisions for everyone?")) ||
      (steps > 1 && !window.confirm(`Undo ${steps} decisions for everyone in this game?`))
    )
      return;
    setHistoryBusy(true);
    setHistoryError(null);
    void changeHistory(action)
      .then(() => {
        setSelectedOptionId(undefined);
        setSelectedSystemId(null);
        setCardSubject(null);
      })
      .catch((error: unknown) =>
        setHistoryError(error instanceof Error ? error.message : String(error)),
      )
      .finally(() => setHistoryBusy(false));
  };
  const userSeat = viewer.role === "player" ? viewer.seat : undefined;
  const [selectedOptionId, setSelectedOptionId] = useState<string>();
  const [selectedSystemId, setSelectedSystemId] = useState<string | null>(null);
  const [cardSubject, setCardSubject] = useState<CardSubject | null>(null);
  const [isTechModalOpen, setIsTechModalOpen] = useState(false);
  const [isObjectivesModalOpen, setIsObjectivesModalOpen] = useState(false);
  useEffect(() => setSelectedOptionId(undefined), [pendingChoice?.nonce]);
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
    const match = pendingChoice.options.find((option) =>
      planetId
        ? option.payload?.planet === planetId ||
          option.id === `exhaust|${planetId}` ||
          option.id === planetId ||
          option.id.startsWith(`exhaust|${planetId}|`)
        : String(option.payload?.system ?? option.payload?.to ?? option.id) === systemId,
    );
    if (!match) {
      setSelectedOptionId(undefined);
      return;
    }
    setSelectedOptionId(match.id);
  };
  return (
    <PlayerIdentityProvider lobby={lobby} seatingOrder={snapshot?.view.seating_order ?? []}>
      {historyError && (
        <div className="session-error" role="alert">
          {historyError}
        </div>
      )}
      <GameShell
        header={
          <div className="game-header">
            <TurnStatusBar
              status={turnStatus}
              view={snapshot?.view ?? null}
              gameVersion={gameVersion}
              connectionStatus={status}
              userSeat={userSeat}
            />
            <div style={{ display: "flex", gap: "8px", alignItems: "center" }}>
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
            </div>
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
              onSelectSystem={(id) => {
                setSelectedSystemId(id);
                setCardSubject(null);
                if (id && pendingChoice && pendingChoice.actor === userSeat) {
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
        activeSystemId={
          typeof snapshot?.state.active_system === "string" ? snapshot.state.active_system : null
        }
        playerSheet={
          snapshot ? (
            <PlayerSheet
              players={snapshot.view.players}
              userSeat={userSeat}
              revealedObjectives={snapshot.view.table.revealed_objectives}
              board={snapshot.view.board}
              onInspectCard={(subject) => {
                setSelectedSystemId(null);
                setCardSubject(subject);
              }}
            />
          ) : null
        }
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
        onChangeHistory={userSeat === lobby.host_player_id ? onChangeHistory : undefined}
        choice={pendingChoice}
        viewerSeat={userSeat}
        players={snapshot?.view.players}
        revealedObjectives={snapshot?.view.table.revealed_objectives}
        scoredObjectives={snapshot?.view.table.scored_objectives}
        objectiveProgress={snapshot?.view.table.objective_progress}
        onSubmitChoice={submitChoice}
        onSubmitMovementBatch={submitMovementBatch}
        onSubmitBasketBatch={submitBatch}
        lastError={lastError}
        selectedOptionId={selectedOptionId}
        onSelectOption={setSelectedOptionId}
      />
      <TechnologyModal
        isOpen={isTechModalOpen}
        onClose={() => setIsTechModalOpen(false)}
        players={snapshot?.view.players}
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
        players={snapshot?.view.players}
        viewerSeat={userSeat}
        onInspectCard={(subject) => {
          setSelectedSystemId(null);
          setCardSubject(subject);
        }}
      />
    </PlayerIdentityProvider>
  );
};
