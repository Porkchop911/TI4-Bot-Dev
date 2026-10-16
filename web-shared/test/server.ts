import { type ChildProcess, spawn } from "node:child_process";
import { existsSync, mkdtempSync, rmSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { dirname, join, resolve } from "node:path";

// A real ti4-server for the tests of both apps (Node only): web2's contract tests and web's
// offline e2e. Each server has a free loopback port and a temp data directory of its own, and is
// ready once it printed `Listening on`. Nothing here is built: a test without a binary is skipped
// by its caller (`testServerBinary()` is null).

/** A running test server. */
export interface TestServer {
  /** `http://127.0.0.1:<port>`. */
  url: string;
  port: number;
  /** The server's `TI4_DATA_DIR`, removed by `stop()`. */
  dataDir: string;
  /** Stops the server, waits for its exit and removes its data directory. */
  stop(): Promise<void>;
}

export interface TestServerOptions {
  /** The server binary. Default: `testServerBinary()`. */
  binary?: string;
  /** More environment for the server (it also gets the caller's). */
  env?: Record<string, string>;
  /** How long the server may take to listen. Default 60 s. */
  timeoutMs?: number;
}

/** The repository: the first directory up from the working directory that has `web-shared/`. */
function repository(): string {
  let dir = process.cwd();
  while (!existsSync(join(dir, "web-shared"))) {
    const up = dirname(dir);
    if (up === dir) {
      throw new Error(`no repository above ${process.cwd()}`);
    }
    dir = up;
  }
  return dir;
}

/**
 * The server binary of the tests: `TI4_E2E_SERVER_BIN`, else the debug and then the release
 * `server` of `$CARGO_TARGET_DIR` (default `target/` of the repository; `cargo b-web` builds the
 * debug one). Null when there is none.
 */
export function testServerBinary(): string | null {
  const set = process.env.TI4_E2E_SERVER_BIN;
  if (set) {
    return resolve(set);
  }
  const root = repository();
  const target = resolve(root, process.env.CARGO_TARGET_DIR ?? "target");
  for (const profile of ["debug", "release"]) {
    const candidate = join(target, profile, "server");
    if (existsSync(candidate)) {
      return candidate;
    }
  }
  return null;
}

const freePort = (): Promise<number> =>
  new Promise((done, fail) => {
    const probe = createServer();
    probe.once("error", fail);
    probe.listen(0, "127.0.0.1", () => {
      const { port } = probe.address() as { port: number };
      probe.close(() => done(port));
    });
  });

const exited = (child: ChildProcess): Promise<void> =>
  child.exitCode !== null || child.signalCode !== null
    ? Promise.resolve()
    : new Promise((done) => child.once("exit", () => done()));

/** Starts a server; it fails (and leaves no process and no directory) when it does not listen. */
export async function spawnTestServer(options: TestServerOptions = {}): Promise<TestServer> {
  const binary = options.binary ?? testServerBinary();
  if (!binary) {
    throw new Error("no server binary: run `cargo b-web`, or set TI4_E2E_SERVER_BIN");
  }
  const port = await freePort();
  const dataDir = mkdtempSync(join(tmpdir(), "ti4-test-server-"));
  const child = spawn(binary, [], {
    env: {
      ...process.env,
      ...options.env,
      HOST: "127.0.0.1",
      PORT: String(port),
      TI4_DATA_DIR: dataDir,
    },
    stdio: ["ignore", "pipe", "pipe"],
  });
  // Both streams are read to the end (a full pipe would block the server); the last output
  // goes into the error of a server that does not start.
  let output = "";
  const keep = (chunk: Buffer) => {
    output = (output + chunk.toString()).slice(-4000);
  };
  child.stdout?.on("data", keep);
  child.stderr?.on("data", keep);
  // A test run that ends without `stop()` leaves no server behind.
  const kill = () => child.kill("SIGKILL");
  process.once("exit", kill);
  let stopped: Promise<void> | null = null;
  const stop = () => {
    stopped ??= (async () => {
      process.off("exit", kill);
      child.kill();
      await exited(child);
      rmSync(dataDir, { recursive: true, force: true });
    })();
    return stopped;
  };
  try {
    await new Promise<void>((done, fail) => {
      const timer = setTimeout(
        () => fail(new Error(`the test server did not listen in time:\n${output}`)),
        options.timeoutMs ?? 60_000,
      );
      const listening = () => {
        if (output.includes("Listening on")) {
          clearTimeout(timer);
          done();
        }
      };
      child.stdout?.on("data", listening);
      child.once("error", (error) => {
        clearTimeout(timer);
        fail(error);
      });
      child.once("exit", (code, signal) => {
        clearTimeout(timer);
        fail(new Error(`the test server stopped (${code ?? signal}):\n${output}`));
      });
    });
  } catch (error) {
    await stop();
    throw error;
  }
  return { url: `http://127.0.0.1:${port}`, port, dataDir, stop };
}
