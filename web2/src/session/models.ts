// The MLP models of single player: the server lists them (`GET /api/models`) and serves their
// files (`GET /api/models/<sha256>.ti4m`, immutable). A file is kept in the Cache API under its
// address, so a browser fetches it once; the engine of the game loads it in its Worker
// (`ti4_model_load`), which checks the SHA-256 again.
import { sha256Hex } from "@ti4/shared/engine/identity.ts";
import type { ModelFile } from "@ti4/shared/engine/protocol.ts";
import type { ModelEntry, TableRequest } from "./wire";

/** A model that the server offers: `id` is the `<name>` of the kind `mlp:<name>`. */
export interface ServedModel {
  id: string;
  label: string;
  /** The SHA-256 of the whole file, in hex: its address. */
  sha256: string;
  /** The size of the file. */
  bytes: number;
}

/** The prefix of the kind of a seat that a model plays (`ti4_host::bots::MLP_PREFIX`). */
export const MLP_PREFIX = "mlp:";

/** The name of the model of a kind, or null for a kind that no model plays. */
export const modelOfKind = (kind: string | null | undefined) =>
  kind?.startsWith(MLP_PREFIX) ? kind.slice(MLP_PREFIX.length) : null;

/** The Cache API store of the files. */
const CACHE = "ti4-models";
export const modelAddress = (sha256: string) => `/api/models/${sha256}.ti4m`;

const isModel = (item: unknown): item is ServedModel => {
  const model = item as ServedModel;
  return (
    !!model &&
    typeof model.id === "string" &&
    typeof model.label === "string" &&
    typeof model.sha256 === "string" &&
    /^[0-9a-f]{64}$/.test(model.sha256) &&
    typeof model.bytes === "number"
  );
};

/**
 * The models that the server offers. Null when there is no server, or its answer is not a list.
 */
export async function listModels(fetcher: typeof fetch = fetch): Promise<ServedModel[] | null> {
  try {
    const response = await fetcher("/api/models", { cache: "no-cache" });
    if (!response.ok) {
      return null;
    }
    const list: unknown = await response.json();
    return Array.isArray(list) ? list.filter(isModel) : null;
  } catch {
    return null;
  }
}

/** The models as the setup options of the engine list them. */
export const modelEntries = (models: readonly ServedModel[]): ModelEntry[] =>
  models.map((model) => ({ name: model.id, label: model.label }));

/** How far the file of a model is: bytes so far, of all. */
export type ModelProgress = (done: number, total: number) => void;

const cacheOf = async () => {
  try {
    return typeof caches === "undefined" ? null : await caches.open(CACHE);
  } catch {
    return null;
  }
};

/** Reads a response whole, telling the progress against the size that the server listed. */
async function readAll(response: Response, total: number, progress?: ModelProgress) {
  const reader = response.body?.getReader();
  if (!reader) {
    return new Uint8Array(await response.arrayBuffer());
  }
  const chunks: Uint8Array[] = [];
  let done = 0;
  progress?.(0, total);
  for (;;) {
    const next = await reader.read();
    if (next.done) {
      break;
    }
    chunks.push(next.value);
    done += next.value.length;
    progress?.(Math.min(done, total), total);
  }
  const bytes = new Uint8Array(done);
  let at = 0;
  for (const chunk of chunks) {
    bytes.set(chunk, at);
    at += chunk.length;
  }
  return bytes;
}

/**
 * The file of a model: from the Cache API when this browser fetched it before, else from the
 * server, checked against its SHA-256 and then kept. Throws when it cannot be had.
 */
export async function modelFile(
  model: ServedModel,
  progress?: ModelProgress,
  fetcher: typeof fetch = fetch,
): Promise<Uint8Array> {
  const address = modelAddress(model.sha256);
  const cache = await cacheOf();
  const kept = await cache?.match(address).catch(() => undefined);
  if (kept) {
    const bytes = new Uint8Array(await kept.arrayBuffer());
    if ((await sha256Hex(bytes)) === model.sha256) {
      progress?.(bytes.length, bytes.length);
      return bytes;
    }
    await cache?.delete(address);
  }
  const response = await fetcher(address);
  if (!response.ok) {
    throw new Error(`The server did not give the file of ${model.label} (${response.status}).`);
  }
  const bytes = await readAll(response, model.bytes, progress);
  if ((await sha256Hex(bytes)) !== model.sha256) {
    throw new Error(`The file of ${model.label} is not the one that the server lists.`);
  }
  await cache
    ?.put(address, new Response(bytes, { headers: { "Content-Type": "application/octet-stream" } }))
    .catch(() => undefined);
  return bytes;
}

/** The names of the models that the seats of a table play, each once. */
export const tableModels = (table: TableRequest | undefined) => [
  ...new Set(
    (table?.seats ?? []).flatMap((seat) => {
      const name = modelOfKind(seat.kind);
      return name ? [name] : [];
    }),
  ),
];

/** A model of a table that the server does not offer (no server, or it dropped the name). */
export class ModelMissing extends Error {}

/**
 * The files of the models that a table seats, for the Worker of its game. Throws `ModelMissing`
 * for a model that the server does not offer.
 */
export async function filesOfTable(
  table: TableRequest | undefined,
  progress?: (model: ServedModel, done: number, total: number) => void,
): Promise<{ model: ServedModel; bytes: Uint8Array }[]> {
  const names = tableModels(table);
  if (!names.length) {
    return [];
  }
  const served = await listModels();
  const files = [];
  for (const name of names) {
    const model = served?.find((item) => item.id === name);
    if (!model) {
      throw new ModelMissing(
        served
          ? `The server no longer offers the model “${name}” that this game seats.`
          : `The model “${name}” that this game seats cannot be had: the server cannot be reached.`,
      );
    }
    files.push({
      model,
      bytes: await modelFile(model, (done, total) => progress?.(model, done, total)),
    });
  }
  return files;
}

/** Copies of the files for one Worker: their buffers are transferred to it. */
export const workerFiles = (files: readonly { model: ServedModel; bytes: Uint8Array }[]) =>
  files.map(({ model, bytes }): ModelFile => ({
    sha256: model.sha256,
    bytes: bytes.slice().buffer,
  }));

/** "12 / 23 MB": what is loaded of a file. */
export function megabytes(done: number, total: number): string {
  const mb = (bytes: number) =>
    total < 10e6 ? (bytes / 1e6).toFixed(1) : String(Math.round(bytes / 1e6));
  return `${mb(done)} / ${mb(total)} MB`;
}
