import React, { useState } from "react";
import { FactionPicker } from "../components/FactionPicker.tsx";
import { preferredNickname } from "../protocol/nickname.ts";
import { buildSetup, DEFAULT_DRAFT, type SetupDraft } from "./setup.ts";
import { PLAYER_COUNTS, type OfflineSetup } from "./types.ts";

/** Setup of a one-human game against rule bots. The seed field exists only in dev builds. */
export const OfflineSetupForm: React.FC<{
  factions: readonly string[];
  onStart: (setup: OfflineSetup) => void;
  disabled?: boolean;
  allowSeed?: boolean;
  initial?: Partial<SetupDraft>;
}> = ({ factions, onStart, disabled, allowSeed = import.meta.env.DEV, initial }) => {
  const [draft, setDraft] = useState<SetupDraft>(() => ({
    ...DEFAULT_DRAFT,
    nickname: safeNickname(),
    ...initial,
  }));
  const [error, setError] = useState<string | null>(null);
  const set = (patch: Partial<SetupDraft>) => setDraft((d) => ({ ...d, ...patch }));
  const submit = (event: React.FormEvent) => {
    event.preventDefault();
    const result = buildSetup(draft, allowSeed, factions);
    if (!result.ok) return setError(result.error);
    setError(null);
    onStart(result.setup);
  };
  const seats = Array.from({ length: draft.playerCount }, (_, i) => i + 1);
  return (
    <form className="lobby-form offline-form" noValidate onSubmit={submit} data-testid="offline-setup-form">
      <label className="field-label">
        Players
        <select
          className="input"
          data-testid="offline-player-count"
          disabled={disabled}
          value={draft.playerCount}
          onChange={(e) => {
            const playerCount = Number(e.target.value);
            set({ playerCount, seat: Number(draft.seat) > playerCount ? "random" : draft.seat });
          }}
        >
          {PLAYER_COUNTS.map((n) => (
            <option key={n} value={n}>
              {n} ({n - 1} bots)
            </option>
          ))}
        </select>
      </label>
      <label className="field-label">
        Your seat
        <select
          className="input"
          data-testid="offline-seat"
          disabled={disabled}
          value={draft.seat}
          onChange={(e) => set({ seat: e.target.value })}
        >
          <option value="random">Random</option>
          {seats.map((s) => (
            <option key={s} value={s}>
              Seat {s}
              {s === 1 ? " (speaker)" : ""}
            </option>
          ))}
        </select>
      </label>
      <div className="field-label">
        Your faction
        <FactionPicker
          pool={factions}
          value={draft.faction}
          onChange={(faction) => set({ faction })}
          disabled={disabled}
          label="Your faction"
          testId="offline-faction"
        />
      </div>
      <label className="field-label">
        Nickname
        <input
          className="input"
          data-testid="offline-nickname"
          disabled={disabled}
          value={draft.nickname}
          onChange={(e) => set({ nickname: e.target.value })}
        />
      </label>
      {allowSeed && (
        <label className="field-label">
          Seed (dev, optional)
          <input
            className="input"
            data-testid="offline-seed"
            disabled={disabled}
            type="number"
            min="0"
            step="1"
            value={draft.seed}
            onChange={(e) => set({ seed: e.target.value })}
          />
        </label>
      )}
      <p className="text-faint">
        The other {draft.playerCount - 1} seats are rule bots with random factions. The game is saved on
        this device after every move.
      </p>
      {error && (
        <div className="session-error offline-error" role="alert" data-testid="offline-setup-error">
          {error}
        </div>
      )}
      <button
        type="submit"
        data-testid="offline-start"
        className="button button--success"
        disabled={disabled}
      >
        Start offline game
      </button>
    </form>
  );
};

function safeNickname(): string {
  try {
    return preferredNickname();
  } catch {
    return "";
  }
}
