import { afterEach, describe, expect, it, vi } from "vitest";
import { act, cleanup, fireEvent, render, screen, waitFor } from "@testing-library/react";
import { buildSetup, DEFAULT_DRAFT, progressFraction, durationLabel } from "./setup.ts";
import { OFFLINE_CAPABILITIES, ONLINE_CAPABILITIES, unavailableFeatures, IfCapable, CapabilityProvider } from "./capabilities.ts";
import { OfflineSetupForm } from "./OfflineSetupForm.tsx";
import { OfflineGamesList } from "./OfflineGamesList.tsx";
import { ResumeProgress } from "./ResumeProgress.tsx";
import { OfflineBanner } from "./OfflineBanner.tsx";
import { OfflineHome } from "./OfflineHome.tsx";
import { createMockLauncher, MOCK_FACTIONS } from "./mockLauncher.ts";
import { feedbackRequestBody, routeFeedback } from "./feedbackRoute.ts";
import { feedbackChipText } from "./useFeedbackQueue.ts";
import { registerOfflineLauncher, setActiveOfflineGame } from "./registry.ts";
import type { OfflineFeedbackLocal, OfflineGameSummary, OfflineMismatchPrompt } from "./types.ts";
import { setLoadedEngine } from "./engineInfo.ts";
import { sha256Hex } from "./local/sha256.ts";

afterEach(() => {
  cleanup();
  registerOfflineLauncher(null);
  setActiveOfflineGame(null);
  localStorage.clear();
});

const draft = { ...DEFAULT_DRAFT, nickname: "Ada" };
const game = (over: Partial<OfflineGameSummary> = {}): OfflineGameSummary => ({
  gameId: "off_1", playerCount: 4, humanFaction: "sol", humanSeat: 2, round: 3, phase: "action",
  answers: 100, createdAt: 1, updatedAt: Date.now() / 1000, status: "in_progress", resumeEstimateMs: 9000, ...over,
});

describe("setup validation", () => {
  it("accepts 3 to 6 players only", () => {
    for (const n of [3, 4, 5, 6]) expect(buildSetup({ ...draft, playerCount: n }, false, MOCK_FACTIONS).ok).toBe(true);
    for (const n of [2, 7, 8]) expect(buildSetup({ ...draft, playerCount: n }, false, MOCK_FACTIONS).ok).toBe(false);
  });
  it("random seat and faction are null; chosen ones are checked", () => {
    const r = buildSetup(draft, false, MOCK_FACTIONS);
    expect(r.ok && r.setup).toMatchObject({ humanSeat: null, humanFaction: null });
    expect(buildSetup({ ...draft, seat: "5", playerCount: 4 }, false, MOCK_FACTIONS).ok).toBe(false);
    expect(buildSetup({ ...draft, faction: "nope" }, false, MOCK_FACTIONS).ok).toBe(false);
    const c = buildSetup({ ...draft, seat: "3", faction: "hacan" }, false, MOCK_FACTIONS);
    expect(c.ok && c.setup).toMatchObject({ humanSeat: 3, humanFaction: "hacan" });
  });
  it("ignores the seed unless allowed, rejects bad seeds", () => {
    const ignored = buildSetup({ ...draft, seed: "42" }, false, MOCK_FACTIONS);
    expect(ignored.ok && "seed" in ignored.setup).toBe(false);
    const used = buildSetup({ ...draft, seed: "42" }, true, MOCK_FACTIONS);
    expect(used.ok && used.setup.seed).toBe(42);
    expect(buildSetup({ ...draft, seed: "-1" }, true, MOCK_FACTIONS).ok).toBe(false);
    expect(buildSetup({ ...draft, seed: "1.5" }, true, MOCK_FACTIONS).ok).toBe(false);
  });
  it("needs a nickname", () => expect(buildSetup({ ...draft, nickname: "  " }, false, MOCK_FACTIONS).ok).toBe(false));
  it("progress helpers", () => {
    expect(progressFraction(5, null)).toBeNull();
    expect(progressFraction(50, 200)).toBe(0.25);
    expect(progressFraction(9, 3)).toBe(1);
    expect(durationLabel(12400)).toBe("about 12 s");
    expect(durationLabel(null)).toBe("a few seconds");
  });
});

describe("capabilities", () => {
  it("lists what offline lacks and nothing online", () => {
    expect(unavailableFeatures(ONLINE_CAPABILITIES)).toEqual([]);
    expect(unavailableFeatures(OFFLINE_CAPABILITIES)).toEqual(
      expect.arrayContaining(["Map splice preview", "Turn redo", "Advisor", "Multiplayer and invite links"]),
    );
  });
  it("IfCapable hides children when off", () => {
    render(
      <CapabilityProvider value={OFFLINE_CAPABILITIES}>
        <IfCapable name="turnRedo">redo-button</IfCapable>
        <IfCapable name="turnRedo">second</IfCapable>
      </CapabilityProvider>,
    );
    expect(screen.queryByText("redo-button")).toBeNull();
  });
});

describe("setup form", () => {
  it("hides the seed outside dev and submits a setup", () => {
    const onStart = vi.fn();
    render(<OfflineSetupForm factions={MOCK_FACTIONS} onStart={onStart} allowSeed={false} initial={{ nickname: "Ada" }} />);
    expect(screen.queryByTestId("offline-seed")).toBeNull();
    expect(screen.getAllByRole("option").map((o) => o.textContent)).not.toContain("2 (1 bots)");
    fireEvent.change(screen.getByTestId("offline-player-count"), { target: { value: "6" } });
    fireEvent.click(screen.getByTestId("offline-start"));
    expect(onStart).toHaveBeenCalledWith({ playerCount: 6, humanSeat: null, humanFaction: null, nickname: "Ada" });
  });
  it("shows the seed in dev and reports validation errors", () => {
    const onStart = vi.fn();
    render(<OfflineSetupForm factions={MOCK_FACTIONS} onStart={onStart} allowSeed initial={{ nickname: "Ada" }} />);
    fireEvent.change(screen.getByTestId("offline-seed"), { target: { value: "-3" } });
    fireEvent.click(screen.getByTestId("offline-start"));
    expect(screen.getByTestId("offline-setup-error").textContent).toMatch(/Seed/);
    expect(onStart).not.toHaveBeenCalled();
  });
  it("resets a seat that no longer exists when shrinking the table", () => {
    render(<OfflineSetupForm factions={MOCK_FACTIONS} onStart={vi.fn()} allowSeed={false} initial={{ nickname: "A", playerCount: 6, seat: "6" }} />);
    fireEvent.change(screen.getByTestId("offline-player-count"), { target: { value: "3" } });
    expect((screen.getByTestId("offline-seat") as HTMLSelectElement).value).toBe("random");
  });
});

describe("games list", () => {
  it("empty state", () => {
    render(<OfflineGamesList games={[]} onResume={vi.fn()} onDelete={vi.fn()} onExport={vi.fn()} />);
    expect(screen.getByTestId("offline-games-empty")).toBeTruthy();
  });
  it("resume, export and delete with confirmation", async () => {
    const onResume = vi.fn(), onExport = vi.fn(), onDelete = vi.fn();
    render(<OfflineGamesList games={[game()]} onResume={onResume} onDelete={onDelete} onExport={onExport} />);
    expect(screen.getByTestId("offline-game-row").textContent).toMatch(/Round 3, action/);
    expect(screen.getByTestId("offline-game-row").textContent).toMatch(/about 9 s/);
    fireEvent.click(screen.getByTestId("offline-resume"));
    expect(onResume).toHaveBeenCalledWith("off_1");
    fireEvent.click(screen.getByTestId("offline-export"));
    expect(onExport).toHaveBeenCalledWith("off_1");
    fireEvent.click(screen.getByTestId("offline-delete"));
    expect(onDelete).not.toHaveBeenCalled();
    fireEvent.click(screen.getByTestId("offline-delete-confirm"));
    await waitFor(() => expect(onDelete).toHaveBeenCalledWith("off_1"));
  });
  it("a finished game says View", () => {
    render(<OfflineGamesList games={[game({ status: "finished" })]} onResume={vi.fn()} onDelete={vi.fn()} onExport={vi.fn()} />);
    expect(screen.getByTestId("offline-resume").textContent).toBe("View");
  });
});

describe("progress", () => {
  it("determinate bar with percent", () => {
    render(<ResumeProgress progress={{ phase: "replaying", done: 50, total: 200 }} />);
    expect(screen.getByRole("progressbar").getAttribute("aria-valuenow")).toBe("25");
    expect(screen.getByTestId("offline-progress-text").textContent).toMatch(/move 50 of 200 \(25%\)/);
  });
  it("indeterminate without total", () => {
    render(<ResumeProgress progress={{ phase: "loading-engine", done: 0, total: null }} />);
    expect(screen.getByRole("progressbar").hasAttribute("aria-valuenow")).toBe(false);
  });
});

const LOCAL: OfflineFeedbackLocal = {
  seed: 7, setup: { seed: 7, players: 4 }, answers: ["a", "b"],
  engine: { build: "abc", content_digest: "d" }, wasm_sha256: "ff",
  save: {
    seed: 7, setup: { seed: 7, players: 4 }, answers: ["a", "b"], redo: [], never: [],
    event_times: [], event_versions: [], generation: 0, version: 3, decisions: 9, decisions_hash: "00ff00ff00ff00ff",
  },
};

describe("banner and queued feedback", () => {
  it("names the hidden features and the queue state", async () => {
    const launcher = createMockLauncher({ feedback: [{ state: "queued" }, { state: "sending" }] });
    render(<OfflineBanner capabilities={launcher.capabilities} feedback={launcher.feedback} />);
    expect(screen.getByTestId("offline-missing").textContent).toMatch(/Turn redo.*Advisor/);
    expect(screen.getByTestId("offline-feedback-chip").textContent).toBe("2 feedback reports will be sent");
    await act(async () => {
      await launcher.feedback.enqueue({ category: "bug", text: "x", context: {} });
    });
    expect(screen.getByTestId("offline-feedback-chip").textContent).toBe("3 feedback reports will be sent");
  });
  it("chip text", () => {
    const item = (state: "queued" | "sent" | "failed") => ({ localId: "a", state, attempts: 0, queuedAt: 0, report: { category: "bug" as const, text: "", context: {} } });
    expect(feedbackChipText([item("sent")])).toBeNull(); // delivered long ago
    expect(feedbackChipText([{ ...item("sent"), queuedAt: Date.now() - 1000 }])).toBe("1 feedback report sent");
    expect(feedbackChipText([item("queued")])).toBe("1 feedback report will be sent");
    expect(feedbackChipText([item("failed"), item("queued")])).toMatch(/could not be sent/);
  });
  it("routes reports into the queue only during an offline game", async () => {
    const launcher = createMockLauncher();
    registerOfflineLauncher(launcher);
    const online = vi.fn(async () => ({ ok: true as const, id: "fb_1" }));
    const body = { category: "bug" as const, text: "t", context: { a: 1 } };
    expect(await routeFeedback(body, online)).toEqual({ ok: true, id: "fb_1" });
    setActiveOfflineGame({
      gameId: "off_1",
      local: () => LOCAL,
    });
    const result = await routeFeedback(body, online);
    expect(result).toMatchObject({ ok: true, queued: true });
    expect(online).toHaveBeenCalledTimes(1);
    expect(launcher.feedback.list()[0].report.local).toMatchObject({ seed: 7, answers: ["a", "b"] });
    const queued = launcher.feedback.list()[0].report;
    expect(queued.reportId).toMatch(/^[A-Za-z0-9_-]{8,64}$/);
    // The wire body: local goes into context, the id is the report_id; no local, no change.
    const wire = feedbackRequestBody(queued);
    expect(wire).toMatchObject({ category: "bug", text: "t", report_id: queued.reportId });
    expect(wire.context).toEqual({ a: 1, local: LOCAL });
    expect(feedbackRequestBody({ category: "idea", text: "x", context: { b: 2 } })).toEqual({
      category: "idea", text: "x", context: { b: 2 },
    });
  });
});

describe("home screen", () => {
  it("starts a game with progress, then enters", async () => {
    const launcher = createMockLauncher();
    const onEnter = vi.fn();
    render(<OfflineHome launcher={launcher} onEnter={onEnter} onBack={vi.fn()} />);
    await screen.findByTestId("offline-games-empty");
    fireEvent.change(screen.getByTestId("offline-nickname"), { target: { value: "Ada" } });
    fireEvent.click(screen.getByTestId("offline-start"));
    expect(await screen.findByTestId("offline-progress")).toBeTruthy();
    await waitFor(() => expect(onEnter).toHaveBeenCalledWith(expect.objectContaining({ gameId: "off_1" })), { timeout: 3000 });
  });
  it("cancelling a resume does not enter", async () => {
    const launcher = createMockLauncher({ games: [game()], replayMs: 5000 });
    const onEnter = vi.fn();
    render(<OfflineHome launcher={launcher} onEnter={onEnter} onBack={vi.fn()} />);
    fireEvent.click(await screen.findByTestId("offline-resume"));
    fireEvent.click(await screen.findByTestId("offline-cancel"));
    await screen.findByTestId("offline-setup-form");
    expect(onEnter).not.toHaveBeenCalled();
    expect(screen.queryByTestId("offline-error")).toBeNull();
  });
  it("shows the reason on an unsupported browser and no setup form", async () => {
    const launcher = createMockLauncher({ unsupported: "Needs Chrome or Edge 137 or newer." });
    render(<OfflineHome launcher={launcher} onEnter={vi.fn()} onBack={vi.fn()} />);
    expect((await screen.findByTestId("offline-unsupported")).textContent).toMatch(/137/);
    expect(screen.queryByTestId("offline-setup-form")).toBeNull();
  });
  it("a failed resume shows the error", async () => {
    const launcher = createMockLauncher({ games: [game()], replayMs: 50, failResume: "Save is damaged." });
    render(<OfflineHome launcher={launcher} onEnter={vi.fn()} onBack={vi.fn()} />);
    fireEvent.click(await screen.findByTestId("offline-resume"));
    expect((await screen.findByTestId("offline-error")).textContent).toBe("Save is damaged.");
  });
  it("deleting removes the row", async () => {
    const launcher = createMockLauncher({ games: [game()] });
    render(<OfflineHome launcher={launcher} onEnter={vi.fn()} onBack={vi.fn()} />);
    fireEvent.click(await screen.findByTestId("offline-delete"));
    fireEvent.click(screen.getByTestId("offline-delete-confirm"));
    await screen.findByTestId("offline-games-empty");
  });
});

const PROMPT: OfflineMismatchPrompt = {
  reason: "engine-changed",
  differs: ["wasmSha256"],
  saved: { build: "0.1.0", contentDigest: "d", wasmSha256: "aaaaaaaa".repeat(8) },
  current: { build: "0.1.0", contentDigest: "d", wasmSha256: "bbbbbbbb".repeat(8) },
  title: "This game was saved with another version of the engine",
  message: "The rules engine has changed.",
  choices: [
    { id: "view-only", label: "View only", destructive: false },
    { id: "try-anyway", label: "Try anyway", destructive: false },
    { id: "delete", label: "Delete this game", destructive: true },
  ],
};

describe("saves, import and the engine", () => {
  it("asks what to do with a game of another engine and enters on view only", async () => {
    const launcher = createMockLauncher({ games: [game()], replayMs: 50, mismatch: PROMPT });
    const onEnter = vi.fn();
    render(<OfflineHome launcher={launcher} onEnter={onEnter} onBack={vi.fn()} />);
    fireEvent.click(await screen.findByTestId("offline-resume"));
    const dialog = await screen.findByTestId("offline-mismatch");
    expect(dialog.textContent).toContain("engine aaaaaaaa");
    expect(dialog.textContent).toContain("engine bbbbbbbb");
    expect(onEnter).not.toHaveBeenCalled(); // nothing opens before the choice
    fireEvent.click(screen.getByTestId("offline-mismatch-try-anyway"));
    await waitFor(() => expect(onEnter).toHaveBeenCalled(), { timeout: 3000 });
  });
  it("delete needs a second click and removes the game", async () => {
    const launcher = createMockLauncher({ games: [game()], replayMs: 50, mismatch: PROMPT });
    render(<OfflineHome launcher={launcher} onEnter={vi.fn()} onBack={vi.fn()} />);
    fireEvent.click(await screen.findByTestId("offline-resume"));
    fireEvent.click(await screen.findByTestId("offline-mismatch-delete"));
    expect(await launcher.list()).toHaveLength(1);
    fireEvent.click(screen.getByTestId("offline-mismatch-delete-confirm"));
    await screen.findByTestId("offline-games-empty");
    expect((await screen.findByTestId("offline-notice")).textContent).toMatch(/deleted/);
  });
  it("shows the storage warning as a visible banner, and none when all is well", async () => {
    const warned = createMockLauncher({ storageWarning: "This browser keeps nothing between visits (private window?)." });
    const { unmount } = render(<OfflineHome launcher={warned} onEnter={vi.fn()} onBack={vi.fn()} />);
    expect((await screen.findByTestId("offline-storage-warning")).textContent).toMatch(/private window/);
    unmount();
    render(<OfflineHome launcher={createMockLauncher()} onEnter={vi.fn()} onBack={vi.fn()} />);
    await screen.findByTestId("offline-setup-form");
    expect(screen.queryByTestId("offline-storage-warning")).toBeNull();
  });
  it("imports a game file into the list", async () => {
    const launcher = createMockLauncher();
    render(<OfflineHome launcher={launcher} onEnter={vi.fn()} onBack={vi.fn()} />);
    await screen.findByTestId("offline-games-empty");
    const file = new File(["{}"], "game.json", { type: "application/json" });
    fireEvent.change(screen.getByTestId("import-game-file"), { target: { files: [file] } });
    await waitFor(() => expect(screen.getAllByTestId("offline-game-row")).toHaveLength(1));
    expect(screen.getByTestId("offline-notice").textContent).toMatch(/imported/);
  });
  it("names the loaded engine in the banner and the build footer", async () => {
    const { BuildFooter } = await import("../components/AlphaNotice.tsx");
    setLoadedEngine("0123456789abcdef".repeat(4));
    const launcher = createMockLauncher();
    render(
      <>
        <OfflineBanner capabilities={launcher.capabilities} feedback={launcher.feedback} />
        <BuildFooter />
      </>,
    );
    expect(screen.getByTestId("offline-engine").textContent).toBe("engine 01234567");
    expect(screen.getByTestId("build-footer-engine").textContent).toContain("engine 01234567");
    setLoadedEngine(null);
  });
  it("hashes like sha-256, with and without crypto.subtle", async () => {
    const bytes = new TextEncoder().encode("abc");
    const expected = "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad";
    expect(await sha256Hex(bytes)).toBe(expected);
    const { sha256 } = await import("./local/sha256.ts");
    expect(Array.from(sha256(bytes), (b) => b.toString(16).padStart(2, "0")).join("")).toBe(expected);
    const long = new Uint8Array(1000).fill(97);
    const viaSubtle = await sha256Hex(long);
    expect(Array.from(sha256(long), (b) => b.toString(16).padStart(2, "0")).join("")).toBe(viaSubtle);
  });
});
