import { describe, expect, it } from "vitest";
import { buildFeedbackContext, type FeedbackEnvironment } from "./feedback/context.ts";
import { feedbackVersions, stateRulesVersion, summarizeVersions } from "./versionInfo.ts";

const hosted = { botVersion: 4, botRecorded: true, newestBot: 5, newestRules: 11 };

describe("summarizeVersions", () => {
  it("reads the rules version of the state; a state without the field is the legacy rules", () => {
    expect(stateRulesVersion({ rules_version: 9 })).toBe(9);
    expect(stateRulesVersion({})).toBe(0);
    expect(stateRulesVersion(undefined)).toBe(0);
  });

  it("flags an offline game on older rules and an older bot", () => {
    const s = summarizeVersions({ hosted, state: { rules_version: 9 }, engine: "engine ab12cd34", webCommit: "abc12345" });
    expect(s.mode).toBe("offline");
    expect(s.rules).toMatchObject({ version: 9, newest: 11, older: true });
    expect(s.rules.note).toMatch(/keeps the older rules/);
    expect(s.bot.note).toMatch(/older bot/);
    expect(s.engine).toBe("engine ab12cd34");
  });

  it("is quiet for a game on the newest versions", () => {
    const s = summarizeVersions({ hosted: { ...hosted, botVersion: 5 }, state: { rules_version: 11 } });
    expect(s.rules.older).toBe(false);
    expect(s.rules.note).toBeNull();
    expect(s.bot.note).toBeNull();
  });

  it("notes a save that predates the bot version and the rules version", () => {
    const s = summarizeVersions({ hosted: { ...hosted, botVersion: 1, botRecorded: false }, state: {} });
    expect(s.bot.note).toMatch(/before bot versions/);
    expect(s.rules).toMatchObject({ version: 0, older: true });
    expect(s.rules.note).toMatch(/legacy rules/);
  });

  it("online: the rules version of the state, no bot version, the server commit", () => {
    const s = summarizeVersions({ hosted: null, state: { rules_version: 11 }, serverCommit: "deadbeef" });
    expect(s.mode).toBe("online");
    expect(s.bot.version).toBeNull();
    expect(s.rules).toMatchObject({ version: 11, newest: null, older: false });
    expect(s.serverCommit).toBe("deadbeef");
  });
});

describe("the feedback context", () => {
  const env: FeedbackEnvironment = {
    commit: "dev",
    serverProtocol: null,
    clientProtocol: 4,
    viewport: { width: 1, height: 1, pixelRatio: 1 },
    userAgent: "UA",
    now: new Date("2026-10-18T10:00:00Z"),
    path: "/games/x",
  };
  it("carries the bot and rules version of the game", () => {
    const versions = summarizeVersions({ hosted, state: { rules_version: 9 }, webCommit: "abc12345" });
    const ctx = buildFeedbackContext({ gameId: "x", versions }, env, true);
    expect(ctx.game_versions).toEqual(feedbackVersions(versions));
    expect(ctx.game_versions).toMatchObject({
      bot_version: 4,
      rules_version: 9,
      newest_rules_version: 11,
      rules_older_than_newest: true,
      web_commit: "abc12345",
    });
    expect(ctx).toHaveProperty("web_commit");
  });
  it("leaves them out when the sender unticks the game info", () => {
    const versions = summarizeVersions({ hosted, state: {} });
    expect(buildFeedbackContext({ versions }, env, false)).not.toHaveProperty("game_versions");
  });
});
