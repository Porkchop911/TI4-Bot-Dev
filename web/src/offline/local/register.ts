import { OFFLINE_CAPABILITIES } from "../capabilities.ts";
import { OFFLINE_FACTION_IDS } from "../factions.ts";
import { registerOfflineLauncher } from "../registry.ts";
import {
  createDurableFeedbackQueue,
  httpFeedbackSender,
  openOfflineSaveStore,
  type OfflineSaveStore,
} from "../storage/index.ts";
import type { OfflineFeedbackQueue, OfflineLauncher } from "../types.ts";
import { hasEngineAsset } from "./engineAsset.ts";
import { JSPI_MISSING, jspiSupported } from "./jspi.ts";

/**
 * The launcher as the page registers it at startup: only what the shell needs at once (flags,
 * faction list, the save store, the feedback queue, the JSPI check) lives in the main bundle.
 * Everything that plays a game (the launcher, the worker, the transport) is behind a dynamic
 * import and loads when "Play offline" is used.
 */
export function createLazyLauncher(store: OfflineSaveStore, feedback: OfflineFeedbackQueue): OfflineLauncher {
  let real: Promise<OfflineLauncher> | null = null;
  const launcher = () =>
    (real ??= import("./launcher.ts").then((m) => m.createLocalLauncher({ store, feedback })));
  return {
    capabilities: OFFLINE_CAPABILITIES,
    factions: OFFLINE_FACTION_IDS,
    supported: () => (jspiSupported() ? { ok: true } : { ok: false, reason: JSPI_MISSING }),
    list: () => store.listSummaries(),
    create: async (setup, onProgress, signal) => (await launcher()).create(setup, onProgress, signal),
    resume: async (gameId, onProgress, signal, options) =>
      (await launcher()).resume(gameId, onProgress, signal, options),
    remove: async (gameId) => (await launcher()).remove(gameId),
    exportGame: (gameId) => store.exportGame(gameId),
    feedback,
    storage: {
      status: () => store.status(),
      subscribe: (listener) => store.subscribe(listener),
      importGame: async (file) => {
        const outcome = await store.importGame(file);
        return { id: outcome.id, engineDiffers: outcome.engineDiffers };
      },
    },
  };
}

/**
 * Registers the real offline launcher when this build has the engine file. The dev mock
 * (screenshots, `?offline-mock`) keeps the slot when it was asked for. Opens the save store and
 * the durable feedback queue first: reports left over from an earlier page are sent now, whether
 * or not a game is opened.
 */
export function registerLocalLauncher(): void {
  const mock =
    Boolean((window as unknown as { __TI4_OFFLINE_MOCK__?: unknown }).__TI4_OFFLINE_MOCK__) ||
    new URLSearchParams(window.location.search).has("offline-mock");
  if (mock || !hasEngineAsset()) return;
  void openOfflineSaveStore().then(async (store) => {
    const queue = createDurableFeedbackQueue({ backend: store.feedbackBackend, send: httpFeedbackSender() });
    await queue.start();
    registerOfflineLauncher(createLazyLauncher(store, queue));
  });
}
