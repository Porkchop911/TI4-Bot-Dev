/**
 * Which versions a game plays, for the settings menu and for feedback reports. Display only: the
 * numbers come from what the engine already records with a game (`Setup::bot_version`,
 * `GameState::rules_version`) and from `describe` (the newest this build knows); nothing here
 * feeds back into a game.
 */

/** What a transport that runs the engine on this device knows (`GameTransport.gameVersions`). */
export interface HostedGameVersions {
  /** The rule bot version the game plays. */
  botVersion: number;
  /** False when the saved game predates the field (it then plays bot version 1). */
  botRecorded: boolean;
  /** The newest bot version this engine ships; null when the engine did not say. */
  newestBot: number | null;
  /** The newest rules version this engine knows (`RULES_VERSION`); null when it did not say. */
  newestRules: number | null;
}

export interface VersionSummary {
  mode: "offline" | "online";
  bot: { version: number | null; newest: number | null; note: string | null };
  rules: { version: number; newest: number | null; older: boolean; note: string | null };
  /** The commit of this page's build ("dev" when unstamped). */
  webCommit: string;
  /** `GET /health` of the server (online): its commit; null offline. */
  serverCommit: string | null;
  /** "engine ab12cd34", offline only. */
  engine: string | null;
}

/** The commit this page was built from (`vite.config.ts` stamps it); "dev" when it is not. */
export const WEB_COMMIT: string = typeof __WEB_COMMIT__ === "string" && __WEB_COMMIT__ ? __WEB_COMMIT__ : "dev";

/** The rules version a snapshot's state carries (`rules_version` is left out of the state while 0). */
export function stateRulesVersion(state: Record<string, unknown> | undefined | null): number {
  const value = state?.rules_version;
  return typeof value === "number" && Number.isFinite(value) ? value : 0;
}

export function summarizeVersions(input: {
  hosted: HostedGameVersions | null;
  state: Record<string, unknown> | undefined | null;
  serverCommit?: string | null;
  engine?: string | null;
  webCommit?: string;
}): VersionSummary {
  const { hosted } = input;
  const rulesVersion = stateRulesVersion(input.state);
  const newestRules = hosted?.newestRules ?? null;
  const older = newestRules !== null && rulesVersion < newestRules;
  let botNote: string | null = null;
  if (!hosted) botNote = "The server does not report the bot version of an online game.";
  else if (!hosted.botRecorded)
    botNote = "This game was saved before bot versions were recorded; it plays bot version 1.";
  else if (hosted.newestBot !== null && hosted.botVersion < hosted.newestBot)
    botNote = `This game keeps the older bot; new games play version ${hosted.newestBot}.`;
  let rulesNote: string | null = null;
  if (older)
    rulesNote =
      rulesVersion === 0
        ? "This game keeps the legacy rules it was started with (saved before rules versions), so its log replays."
        : "This game keeps the older rules it was started with, so its log replays.";
  else if (!hosted && rulesVersion === 0)
    rulesNote = "This game runs the legacy rules (no rules version recorded).";
  return {
    mode: hosted ? "offline" : "online",
    bot: { version: hosted?.botVersion ?? null, newest: hosted?.newestBot ?? null, note: botNote },
    rules: { version: rulesVersion, newest: newestRules, older, note: rulesNote },
    webCommit: input.webCommit ?? WEB_COMMIT,
    serverCommit: hosted ? null : (input.serverCommit ?? null),
    engine: hosted ? (input.engine ?? null) : null,
  };
}

/** The fields a feedback report carries about the versions (read explicitly, never spread). */
export function feedbackVersions(summary: VersionSummary): Record<string, unknown> {
  return {
    mode: summary.mode,
    bot_version: summary.bot.version,
    newest_bot_version: summary.bot.newest,
    rules_version: summary.rules.version,
    newest_rules_version: summary.rules.newest,
    rules_older_than_newest: summary.rules.older,
    web_commit: summary.webCommit,
    ...(summary.engine ? { engine: summary.engine } : {}),
  };
}
