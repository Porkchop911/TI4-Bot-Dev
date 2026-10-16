import { useSyncExternalStore } from "react";

/**
 * Shared-password gate of the public playtest. The server answers 401 `access_required` to every
 * API call until the visitor holds the access cookie; one wrapper around `fetch` notices that
 * answer anywhere in the app and flips this store, which the App turns into the password screen.
 */

let required = false;
const listeners = new Set<() => void>();

function setRequired(value: boolean): void {
  if (required === value) return;
  required = value;
  listeners.forEach((listener) => listener());
}

export function markAccessRequired(): void {
  setRequired(true);
}

export function useAccessRequired(): boolean {
  return useSyncExternalStore(
    (listener) => {
      listeners.add(listener);
      return () => listeners.delete(listener);
    },
    () => required,
    () => false,
  );
}

let installed = false;

/** Wraps `window.fetch` once so any 401 `access_required` raises the password screen. */
export function installAccessGate(): void {
  if (installed || typeof window === "undefined" || typeof window.fetch !== "function") return;
  installed = true;
  const original = window.fetch.bind(window);
  window.fetch = async (...args: Parameters<typeof fetch>) => {
    const response = await original(...args);
    if (response.status === 401) {
      void response
        .clone()
        .json()
        .then((body: unknown) => {
          if ((body as { error?: string } | null)?.error === "access_required") markAccessRequired();
        })
        .catch(() => undefined);
    }
    return response;
  };
}

export type PasswordResult = "ok" | "wrong" | "limited" | "error";

/** Trades the shared password for the access cookie. */
export async function submitAccessPassword(password: string): Promise<PasswordResult> {
  try {
    const response = await fetch("/api/access", {
      method: "POST",
      headers: { "content-type": "application/json" },
      body: JSON.stringify({ password }),
    });
    if (response.ok) return "ok";
    if (response.status === 401) return "wrong";
    if (response.status === 429) return "limited";
    return "error";
  } catch {
    return "error";
  }
}

/** Asks the server once whether the gate is closed for this visitor (no secrets involved). */
export async function probeAccess(): Promise<void> {
  try {
    const response = await fetch("/api/access");
    if (!response.ok) return;
    const body = (await response.json()) as { required?: boolean; granted?: boolean };
    if (body.required && !body.granted) markAccessRequired();
  } catch {
    /* no server, or an old one without the endpoint: nothing to gate */
  }
}

/** Test helper. */
export function resetAccessGateForTests(): void {
  required = false;
  listeners.forEach((listener) => listener());
}
