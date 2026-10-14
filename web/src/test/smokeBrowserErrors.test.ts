import { describe, expect, it } from "vitest";
import { startupRetryable, tolerableConsoleError } from "../../e2e/smokeBrowserErrors";

const exhausted = "Failed to load resource: net::ERR_INSUFFICIENT_RESOURCES";

describe("smoke browser errors", () => {
  // Runs 02-2051, 05-2056 to 11-2100: the last seat's module loads were refused and the run died
  // before decision 1.
  it("retries a seat whose dev-server modules ran out of browser resources", () => {
    expect(
      startupRetryable([
        `[seat 3] console: ${exhausted} (http://127.0.0.1:43937/src/components/UnitInfo.tsx)`,
        `[seat 3] console: ${exhausted} (http://127.0.0.1:43937/src/hooks/useToastMute.ts)`,
      ]),
    ).toBe(true);
  });

  it("does not retry a seat that failed for any other reason", () => {
    expect(startupRetryable([])).toBe(false);
    expect(startupRetryable(["[seat 3] TypeError: x is undefined"])).toBe(false);
    expect(
      startupRetryable([
        `[seat 3] console: ${exhausted} (http://127.0.0.1:1/src/App.tsx)`,
        "[seat 3] GET /api/games/g/snapshot 503: failed closed",
      ]),
    ).toBe(false);
    expect(startupRetryable([`[seat 3] console: ${exhausted} (http://127.0.0.1:1/api/games/g/snapshot)`])).toBe(false);
  });

  // Run 04-2053: a refused presence heartbeat failed the run in round 1; the client retries it on
  // the next tick and the in-flight guard keeps it from piling up.
  it("tolerates a refused periodic lobby request", () => {
    expect(tolerableConsoleError(exhausted, "http://127.0.0.1:36956/api/games/g/lobby/heartbeat")).toBe(true);
    expect(tolerableConsoleError(exhausted, "http://127.0.0.1:20439/api/games/g/lobby/join")).toBe(true);
  });

  it("still reports other failures of those requests and refused game requests", () => {
    expect(
      tolerableConsoleError(
        "Failed to load resource: the server responded with a status of 500 ()",
        "http://127.0.0.1:1/api/games/g/lobby/heartbeat",
      ),
    ).toBe(false);
    expect(tolerableConsoleError(exhausted, "http://127.0.0.1:1/api/games/g/snapshot")).toBe(false);
    expect(tolerableConsoleError(exhausted, "http://127.0.0.1:1/src/App.tsx")).toBe(false);
  });
});
