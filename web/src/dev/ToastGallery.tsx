import React, { useEffect, useRef } from "react";
import { AutoResolveToastContainer } from "../components/AutoResolveToast.tsx";
import { useAutoResolveToasts } from "../hooks/useAutoResolveToasts.ts";

/** Example notifications per scenario, as the auto-resolve hook would be called with them. */
export const TOAST_SCENARIOS: Record<string, Array<[decisionType: string, selectedValue: string]>> = {
  single: [["Strategy Card", "Technology"]],
  stack: [
    ["Strategy Card", "Technology"],
    ["System", "Mecatol Rex (#18)"],
    ["Command token pool", "Tactic"],
  ],
};

/**
 * Development only: renders the bottom-left auto-resolve toasts on their own, because nothing in
 * the game triggers them yet. `/dev/toasts?scenario=single|stack`.
 */
export const ToastGallery: React.FC = () => {
  const { toasts, showToast, dismissToast } = useAutoResolveToasts();
  const scenario = new URLSearchParams(window.location.search).get("scenario") ?? "single";
  // Dev StrictMode runs effects twice; show each scenario's notifications once.
  const shown = useRef<string | null>(null);
  useEffect(() => {
    if (shown.current === scenario) return;
    shown.current = scenario;
    for (const [type, value] of TOAST_SCENARIOS[scenario] ?? TOAST_SCENARIOS.single) showToast(type, value);
    // eslint-disable-next-line react-hooks/exhaustive-deps
  }, [scenario]);
  return (
    <main data-testid="toast-gallery" style={{ minHeight: "100vh", padding: 24 }}>
      <h1>Auto-resolve toasts</h1>
      <p>Development only. These notifications appear in the bottom-left corner and dismiss themselves after a few seconds.</p>
      <AutoResolveToastContainer notifications={toasts} onDismiss={dismissToast} />
    </main>
  );
};
