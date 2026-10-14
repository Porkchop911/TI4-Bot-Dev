export type ReplayCopyState =
  | { kind: "idle" }
  | { kind: "busy" }
  | { kind: "copied"; bytes: number }
  | { kind: "downloaded"; filename: string }
  | { kind: "error"; message: string };

export interface ReplayCopyDeps {
  fetchReplay: () => Promise<{ text: string; filename: string }>;
  writeClipboard?: (text: string) => Promise<void>;
  /** Called when the clipboard is unavailable or refuses; saves the text as a file instead. */
  download: (text: string, filename: string) => void;
}

/**
 * Fetches the replay and puts it on the clipboard; if the clipboard is missing or refuses (no
 * permission, insecure origin) it saves a file instead. Never throws: the outcome is the state.
 */
export async function copyReplay(deps: ReplayCopyDeps): Promise<ReplayCopyState> {
  let replay: { text: string; filename: string };
  try {
    replay = await deps.fetchReplay();
  } catch (error) {
    return {
      kind: "error",
      message: `Could not load the replay: ${error instanceof Error ? error.message : String(error)}`,
    };
  }
  try {
    if (!deps.writeClipboard) throw new Error("Clipboard is unavailable");
    await deps.writeClipboard(replay.text);
    return { kind: "copied", bytes: new Blob([replay.text]).size };
  } catch {
    try {
      deps.download(replay.text, replay.filename);
      return { kind: "downloaded", filename: replay.filename };
    } catch (error) {
      return {
        kind: "error",
        message: `Could not copy or save the replay: ${error instanceof Error ? error.message : String(error)}`,
      };
    }
  }
}

/** Short human text for the status line. */
export function replayCopyMessage(state: ReplayCopyState): string {
  switch (state.kind) {
    case "idle":
      return "";
    case "busy":
      return "Loading replay…";
    case "copied":
      return `Replay copied (${Math.max(1, Math.round(state.bytes / 1024))} KB).`;
    case "downloaded":
      return `Clipboard unavailable, saved ${state.filename} instead.`;
    case "error":
      return state.message;
  }
}

export function downloadText(text: string, filename: string): void {
  const url = URL.createObjectURL(new Blob([text], { type: "application/json" }));
  const link = document.createElement("a");
  link.href = url;
  link.download = filename;
  document.body.appendChild(link);
  link.click();
  link.remove();
  URL.revokeObjectURL(url);
}
