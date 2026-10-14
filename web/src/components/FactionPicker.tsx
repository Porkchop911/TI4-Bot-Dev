import React, { useEffect, useId, useRef, useState } from "react";
import {
  FACTION_HUE,
  factionMonogram,
  groupFactions,
  lobbyFactionName,
} from "../presentation/lobbyFactions.ts";

export const FactionSigil: React.FC<{ faction: string | null }> = ({ faction }) => (
  <span
    className="faction-sigil"
    aria-hidden="true"
    style={faction ? { background: FACTION_HUE[faction] ?? "#94a3b8" } : undefined}
  >
    {faction ? factionMonogram(faction) : "?"}
  </span>
);

interface FactionPickerProps {
  /** Factions the server offers, in its order. */
  pool: readonly string[];
  /** The chosen faction, or `null` for Random. */
  value: string | null;
  /** Faction id -> who holds it; those options are disabled and say who. */
  taken?: ReadonlyMap<string, { nickname: string | null }>;
  onChange: (faction: string | null) => void;
  disabled?: boolean;
  label: string;
  testId: string;
  compact?: boolean;
}

/** A closed button showing the choice; opens a list of Random and every faction in the pool. */
export const FactionPicker: React.FC<FactionPickerProps> = ({
  pool,
  value,
  taken,
  onChange,
  disabled,
  label,
  testId,
  compact,
}) => {
  const [open, setOpen] = useState(false);
  const [query, setQuery] = useState("");
  const root = useRef<HTMLDivElement>(null);
  const listId = useId();
  useEffect(() => {
    if (!open) {
      setQuery("");
      return;
    }
    const away = (event: Event) => {
      if (!root.current?.contains(event.target as Node)) setOpen(false);
    };
    const escape = (event: KeyboardEvent) => {
      if (event.key === "Escape") setOpen(false);
    };
    document.addEventListener("pointerdown", away);
    document.addEventListener("keydown", escape);
    return () => {
      document.removeEventListener("pointerdown", away);
      document.removeEventListener("keydown", escape);
    };
  }, [open]);
  const choose = (faction: string | null) => {
    setOpen(false);
    if (faction !== value) onChange(faction);
  };
  const needle = query.trim().toLowerCase();
  const matches = (id: string, name: string) =>
    !needle || id.includes(needle) || name.toLowerCase().includes(needle);
  const option = (faction: string | null) => {
    const holder = faction ? taken?.get(faction) : undefined;
    return (
      <li
        key={faction ?? "random"}
        role="option"
        aria-selected={faction === value}
        aria-disabled={!!holder}
        tabIndex={holder ? -1 : 0}
        className={`faction-picker__option${faction === value ? " is-selected" : ""}${holder ? " is-taken" : ""}`}
        data-testid={`${testId}-option-${faction ?? "random"}`}
        onClick={() => !holder && choose(faction)}
        onKeyDown={(event) => {
          if (!holder && (event.key === "Enter" || event.key === " ")) {
            event.preventDefault();
            choose(faction);
          }
        }}
      >
        <FactionSigil faction={faction} />
        <span>{faction ? lobbyFactionName(faction) : "Random"}</span>
        {holder && (
          <span className="faction-picker__taken">
            taken by {holder.nickname ?? "another seat"}
          </span>
        )}
        {!faction && <span className="faction-picker__taken">dealt by the seed</span>}
      </li>
    );
  };
  return (
    <div
      className={`faction-picker${compact ? " faction-picker--compact" : ""}`}
      ref={root}
      data-testid={testId}
    >
      <button
        type="button"
        className="button button--outline faction-picker__button"
        aria-haspopup="listbox"
        aria-expanded={open}
        aria-controls={open ? listId : undefined}
        aria-label={`${label}: ${value ? lobbyFactionName(value) : "Random"}`}
        disabled={disabled}
        data-testid={`${testId}-button`}
        onClick={() => setOpen((was) => !was)}
      >
        <FactionSigil faction={value} />
        <span className="faction-picker__name">{value ? lobbyFactionName(value) : "Random"}</span>
        <span aria-hidden="true">▾</span>
      </button>
      {open && (
        <div className="faction-picker__list" id={listId}>
          {pool.length > 8 && (
            <input
              className="input faction-picker__search"
              type="search"
              autoFocus
              placeholder="Search factions"
              aria-label="Search factions"
              data-testid={`${testId}-search`}
              value={query}
              onChange={(event) => setQuery(event.target.value)}
            />
          )}
          <ul className="faction-picker__options" role="listbox" aria-label={label}>
            {matches("random", "Random") && option(null)}
            {groupFactions(pool).map((group) => {
              const shown = group.factions.filter((f) => matches(f, lobbyFactionName(f)));
              if (shown.length === 0) return null;
              return (
                <React.Fragment key={group.label}>
                  <li role="presentation" className="faction-picker__group">
                    {group.label}
                  </li>
                  {shown.map((faction) => option(faction))}
                </React.Fragment>
              );
            })}
          </ul>
        </div>
      )}
    </div>
  );
};
