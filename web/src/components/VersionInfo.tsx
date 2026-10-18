import React from "react";
import { Dialog } from "../primitives/index.ts";
import type { VersionSummary } from "../versionInfo.ts";
import "./VersionInfo.css";

/** Bot version, rules version and build of the game on screen: the same list in the phone menu and the desktop dialog. */
export const VersionInfo: React.FC<{ summary: VersionSummary }> = ({ summary }) => {
  const { bot, rules } = summary;
  return (
    <section className="version-info" data-testid="version-info" aria-label="Versions">
      <h3 className="version-info__title">Versions</h3>
      <dl className="version-info__list">
        <dt>Bot</dt>
        <dd data-testid="version-bot">
          {bot.version === null ? "not reported" : `version ${bot.version}`}
          {bot.newest !== null && <span className="version-info__muted"> (newest {bot.newest})</span>}
          {bot.note && (
            <span className="version-info__note" data-testid="version-bot-note">
              {bot.note}
            </span>
          )}
        </dd>
        <dt>Rules</dt>
        <dd data-testid="version-rules" data-older={rules.older ? "true" : "false"}>
          {rules.version === 0 ? "version 0 (legacy)" : `version ${rules.version}`}
          {rules.newest !== null && <span className="version-info__muted"> (newest this build knows: {rules.newest})</span>}
          {rules.note && (
            <span className={`version-info__note${rules.older ? " version-info__note--warn" : ""}`} data-testid="version-rules-note">
              {rules.note}
            </span>
          )}
        </dd>
        <dt>Page build</dt>
        <dd data-testid="version-web-commit">{summary.webCommit}</dd>
        {summary.serverCommit !== null && (
          <>
            <dt>Server build</dt>
            <dd data-testid="version-server-commit">{summary.serverCommit}</dd>
          </>
        )}
        {summary.engine && (
          <>
            <dt>Engine</dt>
            <dd data-testid="version-engine">{summary.engine.replace(/^engine /, "")}</dd>
          </>
        )}
      </dl>
    </section>
  );
};

/** Desktop: the same list in a small dialog (the desktop header has no menu). */
export const VersionInfoDialog: React.FC<{ summary: VersionSummary; onClose: () => void }> = ({ summary, onClose }) => (
  <Dialog.Root open onOpenChange={(open) => !open && onClose()}>
    <Dialog.Content data-testid="version-info-dialog" className="choice-workflow-dialog">
      <div className="panel version-info-dialog">
        <Dialog.Title as="h2" className="choice-workflow-title">
          Game versions
        </Dialog.Title>
        <Dialog.Description className="version-info__lead">
          What this game plays and what this build knows.
        </Dialog.Description>
        <VersionInfo summary={summary} />
        <div className="version-info-dialog__actions">
          <button type="button" className="button button--secondary" data-testid="version-info-close" onClick={onClose}>
            Close
          </button>
        </div>
      </div>
    </Dialog.Content>
  </Dialog.Root>
);
