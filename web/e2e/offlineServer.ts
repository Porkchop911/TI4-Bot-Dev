import { spawn, type ChildProcess } from "node:child_process";
import { existsSync, mkdtempSync, readdirSync, readFileSync, rmSync, statSync } from "node:fs";
import { createServer } from "node:net";
import { tmpdir } from "node:os";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

/**
 * A real ti4-server on a free loopback port with its own temp data directory, so the offline
 * e2e can prove that a queued feedback report reaches the server's feedback directory exactly
 * once. Uses TI4_E2E_SERVER_BIN, else the debug binary of the workspace target dir.
 */
export interface TestServer {
  url: string;
  dataDir: string;
  stop(): Promise<void>;
  /** Every stored report (parsed JSON), the `.ids` markers of the idempotency check left out. */
  reports(): Array<Record<string, unknown>>;
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

function serverBinary(): string {
  if (process.env.TI4_E2E_SERVER_BIN) return process.env.TI4_E2E_SERVER_BIN;
  const target = process.env.CARGO_TARGET_DIR ?? fileURLToPath(new URL("../../target", import.meta.url));
  for (const profile of ["debug", "release"]) {
    const candidate = join(target, profile, "server");
    if (existsSync(candidate)) return candidate;
  }
  throw new Error("no server binary: build one (cargo build -p ti4-server --bin server) or set TI4_E2E_SERVER_BIN");
}

export async function startTestServer(): Promise<TestServer> {
  const port = await freePort();
  const dataDir = mkdtempSync(join(tmpdir(), "ti4-offline-feedback-"));
  const child: ChildProcess = spawn(serverBinary(), [], {
    env: { ...process.env, PORT: String(port), HOST: "127.0.0.1", TI4_DATA_DIR: dataDir },
    stdio: ["ignore", "pipe", "pipe"],
  });
  await new Promise<void>((done, fail) => {
    const timer = setTimeout(() => fail(new Error("the test server did not start")), 60_000);
    child.stdout?.on("data", (chunk: Buffer) => {
      if (chunk.toString().includes(`Listening on:`)) {
        clearTimeout(timer);
        done();
      }
    });
    child.once("exit", (code) => fail(new Error(`the test server exited early (${code})`)));
  });
  const walk = (dir: string): string[] =>
    readdirSync(dir).flatMap((name) => {
      if (name.startsWith(".")) return [];
      const path = join(dir, name);
      return statSync(path).isDirectory() ? walk(path) : path.endsWith(".json") ? [path] : [];
    });
  return {
    url: `http://127.0.0.1:${port}`,
    dataDir,
    reports: () => {
      const root = join(dataDir, "feedback");
      return existsSync(root) ? walk(root).map((file) => JSON.parse(readFileSync(file, "utf8"))) : [];
    },
    async stop() {
      child.kill();
      rmSync(dataDir, { recursive: true, force: true });
    },
  };
}
