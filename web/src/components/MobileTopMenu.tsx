import React, { useState } from "react";
import { Drawer } from "../primitives/index.ts";
import { setLayoutOverride, useLayoutOverride, type LayoutOverride } from "../hooks/useCompactLayout.ts";
import { useMobileMenuSlot } from "../presentation/MobileMenuSlot.tsx";
import "./MobileTopMenu.css";

/** Why the menu button carries a dot: something wants the viewer, or the link is down. */
export type MenuAttention = "decision" | "turn" | "offline" | null;

/** Which cue the menu button shows: offline first, then a decision waiting for you, then merely your turn. */
export function menuAttention(input: {
  connected: boolean;
  yourDecision: boolean;
  yourTurn: boolean;
}): MenuAttention {
  if (!input.connected) return "offline";
  if (input.yourDecision) return "decision";
  if (input.yourTurn) return "turn";
  return null;
}

const LAYOUT_CHOICES: Array<[LayoutOverride, string]> = [
  [null, "Auto"],
  ["compact", "Phone"],
  ["desktop", "Desktop"],
];

const ATTENTION_LABEL: Record<Exclude<MenuAttention, null>, string> = {
  decision: "a decision is waiting for you",
  turn: "it is your turn",
  offline: "the connection is down",
};

export interface MobileTopMenuProps {
  attention: MenuAttention;
  /** Round, phase, speaker and the full status line. */
  status: React.ReactNode;
  onOpenTechnologies: () => void;
  onOpenObjectives: () => void;
  /** Present on the public playtest server: opens the feedback sheet. */
  onOpenFeedback?: () => void;
}

/**
 * The phone's one menu: a button for the slim top bar and the bottom sheet it opens. The sheet lists
 * the status details, the board's view/zoom/who's-who controls (portaled into the slot) and the
 * Technologies / Objectives buttons. Every row is at least 44 px high.
 */
export const MobileTopMenu: React.FC<MobileTopMenuProps> = ({
  attention,
  status,
  onOpenTechnologies,
  onOpenObjectives,
  onOpenFeedback,
}) => {
  const [open, setOpen] = useState(false);
  const slot = useMobileMenuSlot();
  const layout = useLayoutOverride();
  const close = () => setOpen(false);
  const openThen = (action: () => void) => () => {
    close();
    action();
  };
  return (
    <>
      <button
        type="button"
        data-testid="top-menu-button"
        className="top-menu-button"
        aria-haspopup="dialog"
        aria-expanded={open}
        aria-label={attention ? `Menu, ${ATTENTION_LABEL[attention]}` : "Menu"}
        onClick={() => setOpen(true)}
      >
        <svg width="20" height="20" viewBox="0 0 20 20" aria-hidden="true">
          <path d="M3 5h14M3 10h14M3 15h14" stroke="currentColor" strokeWidth="2" strokeLinecap="round" />
        </svg>
        {attention && (
          <span className="top-menu-button__dot" data-testid="top-menu-dot" data-kind={attention} />
        )}
      </button>
      <Drawer
        open={open}
        onClose={close}
        position="bottom"
        title="Menu"
        data-testid="top-menu-sheet"
        className="top-menu-sheet"
      >
        <div className="top-menu">
          <div className="top-menu__head">
            <span className="top-menu__title">Menu</span>
            <button type="button" data-testid="top-menu-close" className="button button--secondary top-menu__close" onClick={close}>
              Close
            </button>
          </div>
          <div className="top-menu__status" data-testid="top-menu-status">
            {status}
          </div>
          <div className="top-menu__slot" ref={slot?.setSlot} data-testid="top-menu-slot" />
          <div className="top-menu__links">
            <button
              type="button"
              data-testid="technology-modal-button"
              className="button button--secondary"
              onClick={openThen(onOpenTechnologies)}
            >
              Technologies
            </button>
            <button
              type="button"
              data-testid="objectives-modal-button"
              className="button button--secondary"
              onClick={openThen(onOpenObjectives)}
            >
              Objectives
            </button>
            {onOpenFeedback && (
              <button
                type="button"
                data-testid="feedback-menu-button"
                className="button button--secondary"
                onClick={openThen(onOpenFeedback)}
              >
                Send feedback
              </button>
            )}
          </div>
          <div className="top-menu__layout" role="group" aria-label="Layout" data-testid="top-menu-layout">
            <span className="top-menu__layout-label">Layout</span>
            {LAYOUT_CHOICES.map(([value, label]) => (
              <button
                key={label}
                type="button"
                data-testid={`top-menu-layout-${value ?? "auto"}`}
                className={`button button--secondary top-menu__layout-choice${layout === value ? " is-active" : ""}`}
                aria-pressed={layout === value}
                onClick={() => setLayoutOverride(value)}
              >
                {label}
              </button>
            ))}
          </div>
        </div>
      </Drawer>
    </>
  );
};
