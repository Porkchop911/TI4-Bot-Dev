import React from "react";
import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { PasswordScreen } from "./PasswordScreen.tsx";
import { installAccessGate, resetAccessGateForTests, useAccessRequired } from "./accessGate.ts";

afterEach(() => {
  cleanup();
  resetAccessGateForTests();
});

const answer = (status: number, body: unknown = {}) =>
  vi.fn(async () => new Response(JSON.stringify(body), { status })) as unknown as typeof fetch;

describe("PasswordScreen", () => {
  it("posts the password and continues on success", async () => {
    const fetchMock = answer(200, { ok: true });
    globalThis.fetch = fetchMock;
    const granted = vi.fn();
    render(<PasswordScreen onGranted={granted} />);
    expect(screen.getByRole("heading", { name: "Enter the playtest password" })).toBeInTheDocument();
    expect(screen.getByTestId("password-submit")).toBeDisabled();
    fireEvent.change(screen.getByTestId("password-input"), { target: { value: "hunter2" } });
    fireEvent.click(screen.getByTestId("password-submit"));
    await waitFor(() => expect(granted).toHaveBeenCalled());
    expect(fetchMock).toHaveBeenCalledWith(
      "/api/access",
      expect.objectContaining({ method: "POST", body: JSON.stringify({ password: "hunter2" }) }),
    );
  });

  it("says when the password is wrong and when the attempts are used up", async () => {
    globalThis.fetch = answer(401, { error: "invalid_password" });
    const granted = vi.fn();
    render(<PasswordScreen onGranted={granted} />);
    fireEvent.change(screen.getByTestId("password-input"), { target: { value: "nope" } });
    fireEvent.click(screen.getByTestId("password-submit"));
    expect(await screen.findByTestId("password-error")).toHaveTextContent("not right");
    globalThis.fetch = answer(429, { error: "rate_limited" });
    fireEvent.click(screen.getByTestId("password-submit"));
    await waitFor(() => expect(screen.getByTestId("password-error")).toHaveTextContent("Too many tries"));
    expect(granted).not.toHaveBeenCalled();
  });

  it("uses a password field that does not zoom iOS", () => {
    render(<PasswordScreen onGranted={() => undefined} />);
    expect(screen.getByTestId("password-input")).toHaveAttribute("type", "password");
  });
});

describe("access gate", () => {
  it("raises the password screen when any API answer is 401 access_required", async () => {
    const original = window.fetch;
    window.fetch = vi.fn(async () =>
      new Response(JSON.stringify({ error: "access_required" }), { status: 401 }),
    ) as unknown as typeof fetch;
    installAccessGate();
    const Probe: React.FC = () => <span data-testid="probe">{String(useAccessRequired())}</span>;
    render(<Probe />);
    expect(screen.getByTestId("probe")).toHaveTextContent("false");
    await act(async () => {
      await window.fetch("/api/games");
    });
    await waitFor(() => expect(screen.getByTestId("probe")).toHaveTextContent("true"));
    window.fetch = original;
  });
});
