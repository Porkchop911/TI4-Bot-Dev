import { describe, expect, it, vi } from "vitest";
import { copyReplay, replayCopyMessage } from "./replayCopy.ts";

const replay = { text: '{"format":"ti4-replay"}', filename: "ti4-replay-g1.json" };

describe("copyReplay", () => {
  it("copies the fetched text to the clipboard", async () => {
    const writeClipboard = vi.fn().mockResolvedValue(undefined);
    const download = vi.fn();
    const state = await copyReplay({
      fetchReplay: async () => replay,
      writeClipboard,
      download,
    });
    expect(writeClipboard).toHaveBeenCalledWith(replay.text);
    expect(download).not.toHaveBeenCalled();
    expect(state.kind).toBe("copied");
    expect(replayCopyMessage(state)).toMatch(/^Replay copied \(\d+ KB\)\.$/);
  });

  it("saves a file when the clipboard is unavailable", async () => {
    const download = vi.fn();
    const state = await copyReplay({ fetchReplay: async () => replay, download });
    expect(download).toHaveBeenCalledWith(replay.text, replay.filename);
    expect(state).toEqual({ kind: "downloaded", filename: replay.filename });
    expect(replayCopyMessage(state)).toContain(replay.filename);
  });

  it("saves a file when the clipboard refuses", async () => {
    const download = vi.fn();
    const state = await copyReplay({
      fetchReplay: async () => replay,
      writeClipboard: vi.fn().mockRejectedValue(new Error("denied")),
      download,
    });
    expect(state.kind).toBe("downloaded");
    expect(download).toHaveBeenCalledOnce();
  });

  it("reports a server refusal without touching the clipboard", async () => {
    const writeClipboard = vi.fn();
    const state = await copyReplay({
      fetchReplay: async () => {
        throw new Error("the session credential is not valid");
      },
      writeClipboard,
      download: vi.fn(),
    });
    expect(writeClipboard).not.toHaveBeenCalled();
    expect(state.kind).toBe("error");
    expect(replayCopyMessage(state)).toBe(
      "Could not load the replay: the session credential is not valid",
    );
  });

  it("reports an error when neither the clipboard nor the download works", async () => {
    const state = await copyReplay({
      fetchReplay: async () => replay,
      download: () => {
        throw new Error("blocked");
      },
    });
    expect(state).toEqual({
      kind: "error",
      message: "Could not copy or save the replay: blocked",
    });
  });
});
