import React, { useEffect, useRef } from "react";
import { Drawer } from "../primitives/index.ts";

/** Shared, persistent inspection surface for map and card details. */
export const DetailPanel: React.FC<{
  title: string;
  onClose: () => void;
  children: React.ReactNode;
  testId?: string;
  closeTestId?: string;
}> = ({
  title,
  onClose,
  children,
  testId = "detail-panel",
  closeTestId = "close-detail-button",
}) => {
  const closeRef = useRef<HTMLButtonElement>(null);
  useEffect(() => {
    const previous = document.activeElement instanceof HTMLElement ? document.activeElement : null;
    closeRef.current?.focus();
    return () => {
      if (document.activeElement === closeRef.current) previous?.focus();
    };
  }, []);
  return (
    <Drawer
      open
      onClose={onClose}
      modal={false}
      ariaLabel={`${title} details`}
      data-testid={testId}
      className="detail-panel panel"
    >
      <div className="detail-panel__header">
        <h2>{title}</h2>
        <button
          ref={closeRef}
          type="button"
          className="button button--secondary button--icon"
          data-testid={closeTestId}
          aria-label={`Close ${title} details`}
          onClick={onClose}
        >
          ✕
        </button>
      </div>
      <div className="detail-panel__body">{children}</div>
    </Drawer>
  );
};
