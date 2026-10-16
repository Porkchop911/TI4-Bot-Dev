import React, { useSyncExternalStore } from "react";
import type { OfflineStorage } from "./types.ts";

/**
 * Visible notice when games cannot be kept for good (private window, no IndexedDB, quota, a
 * write that failed): the game goes on from memory, so the person must know to export it.
 * Renders nothing when all is well.
 */
export const StorageNotice: React.FC<{ storage: OfflineStorage | undefined }> = ({ storage }) => {
  const warning = useSyncExternalStore(
    (cb) => (storage ? storage.subscribe(cb) : () => undefined),
    () => storage?.status().warning ?? null,
  );
  if (!warning) return null;
  return (
    <div className="offline-storage-notice" role="alert" data-testid="offline-storage-warning">
      <strong>Games are not being saved on this device.</strong> {warning}
    </div>
  );
};
