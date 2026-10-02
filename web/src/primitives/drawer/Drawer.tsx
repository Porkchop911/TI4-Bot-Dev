import React, { useRef, useEffect, useId } from "react";
import { overlayStack } from "../core/overlayStack.ts";
import { useFocusTrap } from "../core/useFocusTrap.ts";

export interface DrawerProps {
  open: boolean;
  onClose: () => void;
  position?: "left" | "right" | "bottom";
  modal?: boolean;
  title?: string;
  ariaLabel?: string;
  children: React.ReactNode;
  className?: string;
  "data-testid"?: string;
  showBackdrop?: boolean;
  style?: React.CSSProperties;
}

export const Drawer: React.FC<DrawerProps> = ({
  open,
  onClose,
  position = "right",
  modal = true,
  title,
  ariaLabel,
  children,
  className,
  "data-testid": testId,
  showBackdrop = true,
  style,
}) => {
  const drawerRef = useRef<HTMLDivElement | null>(null);
  const id = useId();
  const titleId = `drawer-title-${id}`;

  useEffect(() => {
    if (!open) return;
    const unregister = overlayStack.register({
      id: `drawer-${id}`,
      modal,
      element: drawerRef.current,
      onDismiss: onClose,
    });
    return unregister;
  }, [open, modal, id, onClose]);

  useFocusTrap(drawerRef, {
    isActive: open && modal,
  });

  if (!open) return null;

  return (
    <>
      {modal && showBackdrop && (
        <div
          data-testid="drawer-backdrop"
          aria-hidden="true"
          onClick={onClose}
          style={{
            position: "fixed",
            inset: 0,
            background: "rgba(3, 7, 18, 0.6)",
            backdropFilter: "blur(2px)",
            zIndex: "var(--layer-popover)",
          }}
        />
      )}
      <aside
        ref={drawerRef}
        role={modal ? "dialog" : "region"}
        aria-modal={modal ? "true" : undefined}
        aria-label={title ? undefined : ariaLabel}
        aria-labelledby={title ? titleId : undefined}
        tabIndex={-1}
        data-testid={testId || "accessible-drawer"}
        data-position={position}
        className={`accessible-drawer ${className || ""}`}
        onKeyDown={(e) => {
          if (e.key === "Escape") {
            e.preventDefault();
            e.stopPropagation();
            onClose();
          }
        }}
        style={{
          position: modal ? "fixed" : "absolute",
          zIndex: modal ? "var(--layer-dialog)" : "var(--layer-drawer)",
          ...(position === "right"
            ? { top: 0, right: 0, bottom: 0 }
            : position === "left"
              ? { top: 0, left: 0, bottom: 0 }
              : { bottom: 0, left: 0, right: 0 }),
          ...style,
        }}
      >
        {title && (
          <h2
            id={titleId}
            style={{
              position: "absolute",
              width: 1,
              height: 1,
              padding: 0,
              margin: -1,
              overflow: "hidden",
              clip: "rect(0, 0, 0, 0)",
              border: 0,
            }}
          >
            {title}
          </h2>
        )}
        {children}
      </aside>
    </>
  );
};
