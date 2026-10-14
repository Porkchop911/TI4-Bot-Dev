import { useCallback, useState } from "react";

/**
 * How much of a card's printed text a dialog shows. Full text is the default for everyone; a
 * player who knows a card can shrink it (remembered per card name) or switch every card to
 * compact. All storage access is guarded: private windows and blocked storage fall back to
 * the expanded default.
 */

export const COMPACT_KEY = "ti4.cardText.compact";
export const SHRUNK_KEY = "ti4.cardText.shrunk";

function readCompact(): boolean {
  try {
    return window.localStorage.getItem(COMPACT_KEY) === "1";
  } catch {
    return false;
  }
}

function readShrunk(): ReadonlySet<string> {
  try {
    const raw = window.localStorage.getItem(SHRUNK_KEY);
    const parsed: unknown = raw ? JSON.parse(raw) : [];
    return new Set(
      Array.isArray(parsed) ? parsed.filter((name): name is string => typeof name === "string") : [],
    );
  } catch {
    return new Set();
  }
}

function write(key: string, value: string): void {
  try {
    window.localStorage.setItem(key, value);
  } catch {
    // Storage is a convenience only.
  }
}

/** The first sentence of a card's text, or null when the text is a single sentence. */
export function firstSentence(text: string): string | null {
  const trimmed = text.trim();
  const match = /^(.+?[.!?])(?:\s+|$)/s.exec(trimmed);
  if (!match || match[1].length >= trimmed.length) return null;
  return match[1];
}

export interface CardTextPrefs {
  compact: boolean;
  setCompact: (compact: boolean) => void;
  /** Whether the card's text is collapsed to its first sentence right now. */
  isCollapsed: (name: string) => boolean;
  /** Shrink if expanded, show the full text if collapsed. */
  toggle: (name: string) => void;
}

export function useCardTextPrefs(): CardTextPrefs {
  const [compact, setCompactState] = useState<boolean>(readCompact);
  const [shrunk, setShrunk] = useState<ReadonlySet<string>>(readShrunk);
  // Full text opened for this dialog while the compact switch is on; never persisted.
  const [opened, setOpened] = useState<ReadonlySet<string>>(new Set());

  const setCompact = useCallback((value: boolean) => {
    setCompactState(value);
    setOpened(new Set());
    write(COMPACT_KEY, value ? "1" : "0");
  }, []);

  const isCollapsed = useCallback(
    (name: string) => (compact ? !opened.has(name) : shrunk.has(name)),
    [compact, opened, shrunk],
  );

  const toggle = useCallback(
    (name: string) => {
      if (compact) {
        setOpened((previous) => {
          const next = new Set(previous);
          if (next.has(name)) next.delete(name);
          else next.add(name);
          return next;
        });
        return;
      }
      setShrunk((previous) => {
        const next = new Set(previous);
        if (next.has(name)) next.delete(name);
        else next.add(name);
        write(SHRUNK_KEY, JSON.stringify([...next]));
        return next;
      });
    },
    [compact],
  );

  return { compact, setCompact, isCollapsed, toggle };
}
