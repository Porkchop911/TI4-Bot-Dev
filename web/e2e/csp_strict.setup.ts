import { spawnSync } from "node:child_process";
import { resolve } from "node:path";

/** Builds the production bundle into .e2e-dist-csp (the engine is optional: TI4_WASM_FILE or cargo). */
export default function build() {
  const web = resolve(import.meta.dirname, "..");
  const run = (cmd: string, args: string[]) => {
    const r = spawnSync(cmd, args, { cwd: web, stdio: "inherit", env: process.env });
    if (r.status !== 0) throw new Error(`${cmd} ${args.join(" ")} failed`);
  };
  run("node", ["scripts/build-wasm.mjs"]);
  run("npx", ["vite", "build", "--outDir", ".e2e-dist-csp", "--emptyOutDir"]);
}
