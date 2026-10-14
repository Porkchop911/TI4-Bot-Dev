/** Engine identity comparison and the data of the "saved with another engine" dialog. */
import type { EngineIdentity, EngineMismatchPrompt } from "./types.ts";

export function engineDifferences(saved: EngineIdentity, current: EngineIdentity): EngineMismatchPrompt["differs"] {
  const out: EngineMismatchPrompt["differs"] = [];
  if (saved.build !== current.build) out.push("build");
  if (saved.contentDigest !== current.contentDigest) out.push("contentDigest");
  if (saved.wasmSha256 !== current.wasmSha256) out.push("wasmSha256");
  return out;
}

const CHOICES: EngineMismatchPrompt["choices"] = [
  { id: "view-only", label: "View only", destructive: false },
  { id: "try-anyway", label: "Try anyway", destructive: false },
  { id: "delete", label: "Delete this game", destructive: true },
];

/**
 * Before resuming: null when the saved engine is the current one; else the prompt. The game is
 * not replayed until the person chose (as in web2).
 */
export function engineMismatchPrompt(saved: EngineIdentity, current: EngineIdentity): EngineMismatchPrompt | null {
  const differs = engineDifferences(saved, current);
  if (differs.length === 0) return null;
  return {
    reason: "engine-changed",
    differs,
    saved,
    current,
    title: "This game was saved with another version of the engine",
    message:
      "The rules engine has changed since this game was saved, so replaying it may give different decisions. " +
      "You can look at the saved game without playing on, try to resume it anyway, or delete it.",
    choices: CHOICES,
  };
}

/** After the replay: the first `ask` carries `resume_check`; `ok: false` means the engine's decisions differ from the saved ones. */
export function replayDivergedPrompt(
  resumeCheck: { ok: boolean } | null | undefined,
  saved: EngineIdentity,
  current: EngineIdentity,
): EngineMismatchPrompt | null {
  if (!resumeCheck || resumeCheck.ok) return null;
  return {
    reason: "replay-diverged",
    differs: engineDifferences(saved, current),
    saved,
    current,
    title: "This game no longer replays the same way",
    message:
      "Replaying the saved answers gave different decisions than when the game was saved, so the position may " +
      "not be the one you left. You can view it only, keep playing anyway, or delete the game.",
    choices: CHOICES,
  };
}
