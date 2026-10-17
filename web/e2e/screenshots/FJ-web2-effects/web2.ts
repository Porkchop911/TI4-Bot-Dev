import { type ChildProcess, spawn } from "node:child_process";
import { dirname, resolve } from "node:path";
import { fileURLToPath } from "node:url";
import type { Page } from "@playwright/test";

// web2 is another app than web/: the runner's vite serves web/, so these captures start web2's own
// vite (its config, its port) for the folder, and open a local game of the engine in the page
// (`?scenario=…`, the wasm build of `web2/scripts/build-wasm.sh`).

const WEB2 = resolve(dirname(fileURLToPath(import.meta.url)), "../../../../web2");

export interface Web2Server {
  url: string;
  stop(): void;
}

/** Starts `vite` in web2/ on a free port and resolves when it listens. */
export function startWeb2(): Promise<Web2Server> {
  const port = 21_000 + Math.floor(Math.random() * 20_000);
  const child: ChildProcess = spawn("npx", ["vite", "--host", "127.0.0.1", "--port", String(port), "--strictPort"], {
    cwd: WEB2,
    stdio: ["ignore", "pipe", "pipe"],
    detached: true,
  });
  const stop = () => {
    try {
      if (child.pid) process.kill(-child.pid, "SIGTERM");
    } catch {
      // already gone
    }
  };
  return new Promise((done, fail) => {
    let said = "";
    const timer = setTimeout(() => {
      stop();
      fail(new Error(`web2 vite did not start:\n${said}`));
    }, 30_000);
    const read = (chunk: Buffer) => {
      said += chunk.toString();
      if (new RegExp(`Local:\\s+http://127\\.0\\.0\\.1:${port}/`).test(said.replace(/\x1b\[[0-9;]*m/g, ""))) {
        clearTimeout(timer);
        done({ url: `http://127.0.0.1:${port}`, stop });
      }
    };
    child.stdout?.on("data", read);
    child.stderr?.on("data", read);
    child.on("exit", (code) => {
      clearTimeout(timer);
      fail(new Error(`web2 vite exited (${code}):\n${said}`));
    });
  });
}

/**
 * Wraps the Worker of the engine: every message of the protocol that it sends (the `messages` of
 * an envelope) is kept in `window.__ti4Seen`, and passed through `window.__ti4Edit` when a capture
 * set one. The page reads the edited message as if the engine had sent it.
 */
export async function hookEngine(page: Page) {
  await page.addInitScript(() => {
    const w = window as unknown as Record<string, unknown>;
    w.__ti4Seen = [];
    const Base = window.Worker;
    class Hooked extends Base {
      set onmessage(listener: ((event: MessageEvent) => void) | null) {
        super.onmessage = !listener
          ? null
          : (event: MessageEvent) => {
              const data = event.data as { envelope?: { messages?: unknown[] } } | null;
              const messages = data?.envelope?.messages;
              if (Array.isArray(messages)) {
                const edit = w.__ti4Edit as ((message: unknown) => unknown) | undefined;
                data!.envelope!.messages = messages.map((message) => (edit ? edit(message) : message));
                (w.__ti4Seen as unknown[]).push(...data!.envelope!.messages);
              }
              listener.call(this, event);
            };
      }
      get onmessage() {
        return super.onmessage;
      }
    }
    window.Worker = Hooked as typeof Worker;
  });
}
