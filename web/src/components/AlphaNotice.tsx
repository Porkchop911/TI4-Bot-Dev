import React from "react";
import { useBuildInfo } from "../buildInfo.ts";
import { engineLabel, useLoadedEngine } from "../offline/engineInfo.ts";
import { PROTOCOL_VERSION } from "../protocol/types.ts";
import "../feedback/feedback.css";

/** Lobby banner of the public playtest: it is an alpha, things break between updates. */
export const AlphaNotice: React.FC = () => {
  const info = useBuildInfo();
  if (!info.isPublic) return null;
  return (
    <aside className="alpha-notice" data-testid="alpha-notice" role="note">
      <span className="alpha-notice__badge">Alpha</span>
      <span className="alpha-notice__text">
        Early playtest build: expect bugs, and games may be reset when the server updates. Use the Feedback button to tell
        us what went wrong.
      </span>
    </aside>
  );
};

/** Footer line with the build commit and protocol version, so feedback names the build. */
export const BuildFooter: React.FC = () => {
  const info = useBuildInfo();
  // Set once an offline game has loaded the engine file in this page: feedback names it too.
  const engine = engineLabel(useLoadedEngine());
  if (!info.isPublic && !engine) return null;
  return (
    <footer className="build-footer" data-testid="build-footer">
      {info.isPublic
        ? `Alpha · build ${info.commit} · protocol v${info.protocolVersion ?? PROTOCOL_VERSION}`
        : "Offline"}
      {engine && <span data-testid="build-footer-engine"> · {engine}</span>}
    </footer>
  );
};
