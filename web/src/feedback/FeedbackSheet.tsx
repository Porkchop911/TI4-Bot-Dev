import React, { useState } from "react";
import { Drawer } from "../primitives/index.ts";
import { fetchBuildInfo, useBuildInfo } from "../buildInfo.ts";
import { PROTOCOL_VERSION } from "../protocol/types.ts";
import { buildFeedbackContext, currentEnvironment } from "./context.ts";
import { preferredNickname } from "../protocol/nickname.ts";
import { currentFeedbackSource } from "./source.ts";
import { routeFeedback } from "../offline/feedbackRoute.ts";
import "./feedback.css";

export const MAX_FEEDBACK_CHARS = 4000;
export const MAX_NAME_CHARS = 60;

/** The nickname the roster has for the viewer's seat, if any. */
function knownNickname(): string | undefined {
  const { viewer, lobby } = currentFeedbackSource();
  if (viewer?.role !== "player") return undefined;
  return lobby?.slots.find((slot) => slot.occupant === viewer.seat)?.nickname ?? undefined;
}

function safeStoredNickname(): string | undefined {
  try {
    return preferredNickname() || undefined;
  } catch {
    return undefined;
  }
}

export const CATEGORIES = [
  ["bug", "Bug"],
  ["confusing", "Confusing"],
  ["idea", "Idea"],
  ["other", "Other"],
] as const;
export type FeedbackCategory = (typeof CATEGORIES)[number][0];

export type FeedbackResult = { ok: true; id: string; queued?: boolean } | { ok: false; message: string };

/** The sheet's default: straight to the server, or into the offline queue during an offline game. */
export const submitFeedback = (body: Parameters<typeof sendFeedback>[0]): Promise<FeedbackResult> =>
  routeFeedback(body, sendFeedback);

export async function sendFeedback(body: {
  category: FeedbackCategory;
  text: string;
  context: Record<string, unknown>;
}): Promise<FeedbackResult> {
  try {
    const response = await fetch("/api/feedback", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify(body),
    });
    if (response.ok) {
      const data = (await response.json()) as { id?: string };
      return { ok: true, id: data.id ?? "" };
    }
    if (response.status === 429) return { ok: false, message: "Too many reports in a short time. Try again later." };
    if (response.status === 401) return { ok: false, message: "Enter the playtest password first." };
    if (response.status === 413) return { ok: false, message: "The report is too large." };
    return { ok: false, message: "The server could not save the report. Try again." };
  } catch {
    return { ok: false, message: "Could not reach the server. Your text is kept; try again." };
  }
}

export interface FeedbackSheetProps {
  open: boolean;
  onClose: () => void;
  /** Injectable for tests. */
  send?: typeof sendFeedback;
}

export const FeedbackSheet: React.FC<FeedbackSheetProps> = ({ open, onClose, send = submitFeedback }) => {
  const info = useBuildInfo();
  const [category, setCategory] = useState<FeedbackCategory>("bug");
  const [text, setText] = useState("");
  const [includeGame, setIncludeGame] = useState(true);
  const [typedName, setTypedName] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const [sentId, setSentId] = useState<string | null>(null);
  const [queued, setQueued] = useState(false);
  const trimmed = text.trim();
  const nickname = open ? knownNickname() : undefined;
  const close = () => {
    onClose();
    // A sent report starts the next one from scratch.
    if (sentId) {
      setSentId(null);
      setText("");
    }
  };
  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (busy || !trimmed || text.length > MAX_FEEDBACK_CHARS) return;
    setBusy(true);
    setProblem(null);
    const latest = await fetchBuildInfo();
    const context = buildFeedbackContext(
      currentFeedbackSource(),
      {
        ...currentEnvironment(latest.commit, latest.protocolVersion, PROTOCOL_VERSION),
        storedNickname: safeStoredNickname(),
        typedName: nickname ? undefined : typedName,
      },
      includeGame,
    );
    const result = await send({ category, text: trimmed, context });
    setBusy(false);
    if (result.ok) {
      setSentId(result.id);
      setQueued(result.queued === true);
    }
    else setProblem(result.message);
  };
  return (
    <Drawer
      open={open}
      onClose={close}
      position="bottom"
      title="Send feedback"
      data-testid="feedback-sheet"
      className="feedback-sheet"
    >
      {sentId !== null ? (
        <div className="feedback" data-testid="feedback-sent">
          <h2 className="feedback__title" aria-hidden="true">
            Thank you
          </h2>
          <p className="feedback__sent-text">
            {queued
              ? "Your report is saved on this device and will be sent in the background. You can keep playing. Thank you, it helps."
              : "Your report was saved. Thank you, it helps."}
          </p>
          {queued && (
            <p className="feedback__queued" data-testid="feedback-queued">
              Will be sent
            </p>
          )}
          {sentId && !queued && (
            <p className="feedback__sent-id">
              Report id <code data-testid="feedback-id">{sentId}</code>
            </p>
          )}
          <button type="button" className="button button--primary feedback__submit" data-testid="feedback-done" onClick={close}>
            Close
          </button>
        </div>
      ) : (
        <form className="feedback" onSubmit={(event) => void submit(event)}>
          <div className="feedback__head">
            <h2 className="feedback__title" aria-hidden="true">
              Send feedback
            </h2>
            <button type="button" className="button button--secondary feedback__close" data-testid="feedback-close" onClick={close}>
              Close
            </button>
          </div>
          <div className="feedback__categories" role="radiogroup" aria-label="Category" data-testid="feedback-categories">
            {CATEGORIES.map(([value, label]) => (
              <button
                key={value}
                type="button"
                role="radio"
                aria-checked={category === value}
                data-testid={`feedback-category-${value}`}
                className={`feedback__chip${category === value ? " feedback__chip--on" : ""}`}
                onClick={() => setCategory(value)}
              >
                {label}
              </button>
            ))}
          </div>
          <label className="feedback__label" htmlFor="feedback-text">
            What happened?
          </label>
          <textarea
            id="feedback-text"
            data-testid="feedback-text"
            className="feedback__text"
            rows={5}
            maxLength={MAX_FEEDBACK_CHARS}
            placeholder="Describe what you did, what you expected and what you saw."
            value={text}
            onChange={(event) => setText(event.target.value)}
          />
          <div className="feedback__count" data-testid="feedback-count">
            {text.length} / {MAX_FEEDBACK_CHARS}
          </div>
          {nickname ? (
            <p className="feedback__who" data-testid="feedback-who">
              Sending as <strong>{nickname}</strong>
            </p>
          ) : (
            <>
              <label className="feedback__label" htmlFor="feedback-name">
                Your name (optional)
              </label>
              <input
                id="feedback-name"
                data-testid="feedback-name"
                className="feedback__name"
                type="text"
                maxLength={MAX_NAME_CHARS}
                placeholder="So we know who to ask"
                value={typedName}
                onChange={(event) => setTypedName(event.target.value)}
              />
            </>
          )}
          <label className="feedback__check">
            <input
              type="checkbox"
              data-testid="feedback-include-game"
              checked={includeGame}
              onChange={(event) => setIncludeGame(event.target.checked)}
            />
            <span>Include game info</span>
          </label>
          <p className="feedback__privacy">
            Sent: build {info.commit}, your name{nickname ? "" : " if you typed one"}, your screen size and browser
            {includeGame
              ? ", the game id, your seat and faction, the other players' nicknames, round and phase, the open decision and the last public events. Nothing hidden from you."
              : "."}{" "}
            Stored as a file on the playtest server; ask to have it deleted.
          </p>
          {problem && (
            <div className="feedback__error" role="alert" data-testid="feedback-error">
              {problem}
            </div>
          )}
          <button
            type="submit"
            className="button button--primary feedback__submit"
            data-testid="feedback-submit"
            disabled={busy || !trimmed}
          >
            {busy ? "Sending…" : "Send"}
          </button>
        </form>
      )}
    </Drawer>
  );
};
