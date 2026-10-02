import React, { useState } from "react";
import { GameShell } from "../components/GameShell.tsx";
import { deriveChoiceRendererModel } from "../presentation/choiceModel.ts";
import { Board } from "../components/Board.tsx";
import { PlayerIdentityProvider } from "../presentation/PlayerIdentity.tsx";
import { PROTOCOL_VERSION, type ClientMessage } from "../protocol/types.ts";
import { galleryBoard, galleryLobby, galleryPlayers, gallerySeating } from "./galleryBoard.ts";
import { fallbackCases, galleryCases, type GalleryCase } from "./decisionGalleryCases.ts";
import { galleryEventLog } from "./galleryEventLog.ts";
import "./DecisionGallery.css";

/** Local, synthetic presentation only: submissions never contact a game server. */
export const DecisionGallery: React.FC = () => {
  const [selected, setSelected] = useState<GalleryCase | null>(null);
  const [lastSubmitted, setLastSubmitted] = useState<string | null>(null);
  const [rejectNext, setRejectNext] = useState(false);
  const [viewer, setViewer] = useState<"actor" | "other">("actor");
  const [nonce, setNonce] = useState(0);
  const [selectedOption, setSelectedOption] = useState<string>();
  const [selectedSystemId, setSelectedSystemId] = useState<string | null>(null);
  const [trace, setTrace] = useState<
    Array<{ message: Extract<ClientMessage, { type: "submit_choice" }>; result: string }>
  >([]);
  const [awaiting, setAwaiting] = useState(false);
  const [copied, setCopied] = useState(false);

  const select = (item: GalleryCase | null) => {
    setSelected(item);
    setLastSubmitted(null);
    setRejectNext(false);
    setViewer("actor");
    setSelectedOption(undefined);
    setSelectedSystemId(null);
    setTrace([]);
    setAwaiting(false);
    setNonce((previous) => previous + 1);
  };

  const choice = selected
    ? { ...selected.choice, nonce: `${selected.choice.nonce}-${nonce}` }
    : null;
  const viewerSeat = viewer === "actor" ? selected?.choice.actor : "other_seat";
  const classified = deriveChoiceRendererModel(choice, viewerSeat ?? null)?.workflow;

  return (
    <div className="decision-gallery" data-testid="decision-gallery">
      <header className="decision-gallery__header panel">
        <div
          style={{
            display: "flex",
            justifyContent: "space-between",
            alignItems: "center",
            flexWrap: "wrap",
            gap: 12,
          }}
        >
          <h1>
            Decision gallery <small>Development only · synthetic previews</small>
          </h1>
          <a href="/dev/scenarios" className="button button--secondary">
            Switch to Live Dev Scenarios &rarr;
          </a>
        </div>
        <p>
          No server is connected. Submissions are recorded locally; they do not acknowledge an
          action or advance an engine decision.
        </p>
        {selected && (
          <div className="workflow-row">
            <button type="button" className="button button--secondary" onClick={() => select(null)}>
              All decisions
            </button>
            <button
              type="button"
              className="button button--secondary"
              onClick={() => {
                setNonce((n) => n + 1);
                setLastSubmitted(null);
                setAwaiting(false);
                setSelectedOption(undefined);
                setSelectedSystemId(null);
              }}
            >
              Reset preview
            </button>
            <label>
              <input
                type="checkbox"
                checked={viewer === "other"}
                onChange={(event) => {
                  setViewer(event.target.checked ? "other" : "actor");
                  setNonce((n) => n + 1);
                }}
              />{" "}
              View as another seat
            </label>
            <label>
              <input
                type="checkbox"
                checked={rejectNext}
                onChange={(event) => setRejectNext(event.target.checked)}
              />{" "}
              Reject next submission
            </label>
          </div>
        )}
      </header>
      {!selected ? (
        <main className="decision-gallery__index">
          <h2>Workflow kinds ({galleryCases.length}) · synthetic examples</h2>
          <div className="decision-gallery__grid">
            {galleryCases.map((item) => (
              <button
                type="button"
                key={item.workflow}
                className="workflow-card decision-gallery__case"
                onClick={() => select(item)}
              >
                <strong>{item.title}</strong> <span>{item.choice.context?.subtype}</span>{" "}
                <span>{item.note}</span>
              </button>
            ))}
          </div>
          <h2>Fallbacks and boundary states ({fallbackCases.length})</h2>
          <div className="decision-gallery__grid">
            {fallbackCases.map((item) => (
              <button
                type="button"
                key={item.choice.nonce}
                className="workflow-card decision-gallery__case"
                onClick={() => select(item)}
              >
                <strong>{item.title}</strong> <span>{item.fallback}</span> <span>{item.note}</span>
              </button>
            ))}
          </div>
        </main>
      ) : (
        <main className="decision-gallery__preview">
          <details className="panel decision-gallery__details" aria-label="Gallery debug details">
            <summary>Gallery debug details · synthetic fixture</summary>
            <h2>{selected.title}</h2>
            <p>{selected.note}</p>
            <p>
              Workflow: <code>{classified ?? "(hidden from other seat)"}</code> · Subtype:{" "}
              <code>{choice?.context?.subtype ?? "(none)"}</code>
            </p>
            <p>
              Raw prompt: {choice?.prompt} · Nonce: <code>{choice?.nonce}</code> · Context:{" "}
              <code>{JSON.stringify(choice?.context ?? null)}</code>
            </p>
            {selected.fallback && <p>Fallback: {selected.fallback}</p>}
            <p>
              Offered IDs:{" "}
              {choice?.options.length
                ? choice.options.map((o) => <code key={o.id}>{o.id} </code>)
                : "(none)"}
            </p>
            <p role="status">
              {lastSubmitted
                ? `Local submission: ${lastSubmitted} (no engine transition)`
                : "No local submission yet."}
            </p>
            {awaiting && (
              <p>
                Awaiting next decision — remaining intent is unverified until an authoritative
                offer.
              </p>
            )}
            <h3>Attempted submit_choice messages (gallery placeholders, no server connection)</h3>
            <ol>
              {trace.map((entry, index) => (
                <li key={index}>
                  <code>{JSON.stringify(entry.message)}</code> — {entry.result}
                </li>
              ))}
            </ol>
            {trace.length > 0 && (
              <button
                type="button"
                className="button button--secondary"
                onClick={() => {
                  void navigator.clipboard.writeText(JSON.stringify(trace.at(-1)!.message));
                  setCopied(true);
                }}
              >
                Copy JSON
              </button>
            )}
            {copied && <span role="status">Copied attempted message</span>}
            <p>
              Minimize the decision (or press Escape) to inspect these controls and switch previews.
            </p>
          </details>
          <PlayerIdentityProvider lobby={galleryLobby} seatingOrder={gallerySeating}>
            <GameShell
              key={`${selected.choice.nonce}-${viewer}`}
              header={<span>Synthetic gallery board</span>}
              board={
                <Board
                  board={galleryBoard}
                  players={galleryPlayers}
                  seatingOrder={gallerySeating}
                  pendingChoice={choice}
                  viewerSeat={viewerSeat}
                  selectedSystemId={selectedSystemId}
                  onSelectSystem={(sys) => {
                    setSelectedSystemId(sys);
                    if (!sys) {
                      setSelectedOption(undefined);
                    } else {
                      const offered = choice?.options.find(
                        (o) =>
                          String(o.payload?.system ?? o.id) === sys ||
                          o.id === `activate|${sys}` ||
                          o.id === sys,
                      );
                      if (!offered) {
                        setSelectedOption(undefined);
                      }
                    }
                  }}
                  onSelectOptionId={(optId) => {
                    setSelectedOption(optId);
                  }}
                  onSelectTarget={(system, planet) => {
                    const offered = choice?.options.find((o) =>
                      planet
                        ? o.id === `exhaust|${planet}` ||
                          o.id === planet ||
                          o.payload?.planet === planet
                        : String(o.payload?.system ?? o.id) === system ||
                          o.id === `activate|${system}` ||
                          o.id === system,
                    );
                    if (offered) {
                      setSelectedOption(offered.id);
                    } else {
                      setSelectedOption(undefined);
                    }
                  }}
                />
              }
              playerSheet={
                <div className="panel">Synthetic players: Alex (2 TG), Blair (1 TG)</div>
              }
              events={galleryEventLog}
              currentPath={{
                round: 1,
                phase: "action",
                action_id: "action_1",
                stage: "production",
              }}
              players={galleryPlayers}
              boardView={galleryBoard}
              choice={choice}
              viewerSeat={viewerSeat}
              selectedOptionId={selectedOption}
              selectedSystemId={selectedSystemId}
              onSelectOption={(opt) => {
                setSelectedOption(opt);
                if (!opt) setSelectedSystemId(null);
              }}
              onSubmitChoice={async (id) => {
                if (!choice?.options.some((o) => o.id === id))
                  throw new Error("Option is not offered in this preview.");
                const message = {
                  type: "submit_choice" as const,
                  protocol_version: PROTOCOL_VERSION,
                  game_id: "gallery-placeholder",
                  nonce: choice.nonce,
                  expected_version: 1,
                  option_id: id,
                };
                if (rejectNext) {
                  setRejectNext(false);
                  setTrace((list) => [...list, { message, result: "simulated rejection" }]);
                  throw new Error("Simulated rejection: try again or reset the preview.");
                }
                setTrace((list) => [...list, { message, result: "locally recorded" }]);
                setLastSubmitted(id);
                setAwaiting(true);
                // Do not synthesize a new nonce: a local callback is not an authoritative transition.
              }}
            />
          </PlayerIdentityProvider>
        </main>
      )}
    </div>
  );
};
