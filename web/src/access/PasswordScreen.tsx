import React, { useState } from "react";
import { submitAccessPassword } from "./accessGate.ts";
import "./access.css";

const MESSAGES = {
  wrong: "That password is not right.",
  limited: "Too many tries. Wait a minute and try again.",
  error: "Could not reach the server. Try again.",
} as const;

export const PasswordScreen: React.FC<{ onGranted?: () => void }> = ({
  onGranted = () => window.location.reload(),
}) => {
  const [password, setPassword] = useState("");
  const [busy, setBusy] = useState(false);
  const [problem, setProblem] = useState<string | null>(null);
  const submit = async (event: React.FormEvent) => {
    event.preventDefault();
    if (busy || !password) return;
    setBusy(true);
    setProblem(null);
    const result = await submitAccessPassword(password);
    setBusy(false);
    if (result === "ok") onGranted();
    else setProblem(MESSAGES[result]);
  };
  return (
    <main className="access-screen" data-testid="password-screen">
      <form className="access-screen__card" onSubmit={(event) => void submit(event)}>
        <span className="access-screen__badge">Alpha</span>
        <h1 className="access-screen__title">Enter the playtest password</h1>
        <p className="access-screen__text">
          This is a closed alpha playtest of a Twilight Imperium 4 web client. Ask the organiser for the password.
        </p>
        <label className="access-screen__label" htmlFor="access-password">
          Password
        </label>
        <input
          id="access-password"
          data-testid="password-input"
          className="access-screen__input"
          type="password"
          autoComplete="current-password"
          autoFocus
          value={password}
          onChange={(event) => setPassword(event.target.value)}
        />
        {problem && (
          <div className="access-screen__error" role="alert" data-testid="password-error">
            {problem}
          </div>
        )}
        <button
          type="submit"
          data-testid="password-submit"
          className="button button--primary access-screen__submit"
          disabled={busy || !password}
        >
          {busy ? "Checking…" : "Enter"}
        </button>
      </form>
    </main>
  );
};
