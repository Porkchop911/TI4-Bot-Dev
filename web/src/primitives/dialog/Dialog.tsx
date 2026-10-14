import React, {
  createContext,
  useContext,
  useId,
  useRef,
  useState,
  useEffect,
  forwardRef,
} from "react";
import { overlayStack } from "../core/overlayStack.ts";
import { useFocusTrap } from "../core/useFocusTrap.ts";

interface DialogContextValue {
  isOpen: boolean;
  onOpenChange: (open: boolean) => void;
  modal: boolean;
  dialogId: string;
  titleId: string;
  descriptionId: string;
  hasDescription: boolean;
  setHasDescription: (has: boolean) => void;
}

const DialogContext = createContext<DialogContextValue | null>(null);

export function useDialogContext(): DialogContextValue {
  const ctx = useContext(DialogContext);
  if (!ctx) {
    throw new Error("Dialog compound components must be used within a Dialog.Root");
  }
  return ctx;
}

export interface DialogRootProps {
  open: boolean;
  onOpenChange: (open: boolean) => void;
  modal?: boolean;
  children: React.ReactNode;
}

export const DialogRoot: React.FC<DialogRootProps> = ({
  open,
  onOpenChange,
  modal = true,
  children,
}) => {
  const dialogId = useId();
  const titleId = `${dialogId}-title`;
  const descriptionId = `${dialogId}-desc`;
  const [hasDescription, setHasDescription] = useState(false);

  return (
    <DialogContext.Provider
      value={{
        isOpen: open,
        onOpenChange,
        modal,
        dialogId,
        titleId,
        descriptionId,
        hasDescription,
        setHasDescription,
      }}
    >
      {children}
    </DialogContext.Provider>
  );
};

export interface DialogContentProps extends React.HTMLAttributes<HTMLDivElement> {
  children: React.ReactNode;
  "data-testid"?: string;
  preventCloseOnEscape?: boolean;
  onEscape?: () => void;
  initialFocusRef?: React.RefObject<HTMLElement | null>;
  returnFocusRef?: React.RefObject<HTMLElement | null>;
  keepMounted?: boolean;
}

export const DialogContent = forwardRef<HTMLDivElement, DialogContentProps>(
  (
    {
      children,
      className,
      style,
      "data-testid": testId,
      preventCloseOnEscape = false,
      onEscape,
      initialFocusRef,
      returnFocusRef,
      keepMounted = false,
      ...rest
    },
    forwardedRef,
  ) => {
    const { isOpen, onOpenChange, modal, titleId, descriptionId, hasDescription } =
      useDialogContext();
    const internalRef = useRef<HTMLDivElement | null>(null);

    const setRef = (node: HTMLDivElement | null) => {
      internalRef.current = node;
      if (typeof forwardedRef === "function") {
        forwardedRef(node);
      } else if (forwardedRef && "current" in forwardedRef) {
        (forwardedRef as React.MutableRefObject<HTMLDivElement | null>).current = node;
      }
    };

    // Register with overlayStack for LIFO Escape management
    useEffect(() => {
      if (!isOpen) return;
      const unregister = overlayStack.register({
        id: titleId,
        modal,
        element: internalRef.current,
        onDismiss: () => {
          if (preventCloseOnEscape) {
            onEscape?.();
            return;
          }
          if (onEscape) {
            onEscape();
          } else {
            onOpenChange(false);
          }
        },
      });
      return unregister;
    }, [isOpen, modal, titleId, preventCloseOnEscape, onEscape, onOpenChange]);

    useFocusTrap(internalRef, {
      isActive: isOpen && modal,
      initialFocusRef,
      returnFocusRef,
    });

    if (!isOpen && !keepMounted) return null;

    const ariaDescribedBy =
      rest["aria-describedby"] || (hasDescription ? descriptionId : undefined);

    return (
      <div
        ref={setRef}
        role="dialog"
        tabIndex={-1}
        aria-modal={modal ? "true" : undefined}
        aria-labelledby={titleId}
        aria-describedby={ariaDescribedBy}
        data-testid={testId}
        className={className}
        style={{ ...style, display: isOpen ? style?.display : "none" }}
        onKeyDown={(e) => {
          rest.onKeyDown?.(e);
          if (e.key === "Escape" && !e.defaultPrevented) {
            e.preventDefault();
            if (preventCloseOnEscape) {
              onEscape?.();
            } else if (onEscape) {
              onEscape();
            } else {
              onOpenChange(false);
            }
          }
        }}
        {...rest}
      >
        {children}
      </div>
    );
  },
);

DialogContent.displayName = "DialogContent";

export interface DialogTitleProps extends React.HTMLAttributes<HTMLHeadingElement> {
  children: React.ReactNode;
  as?: "h1" | "h2" | "h3" | "h4" | "h5" | "h6" | "div";
}

export const DialogTitle: React.FC<DialogTitleProps> = ({
  children,
  as: Component = "h2",
  id: customId,
  ...rest
}) => {
  const { titleId } = useDialogContext();
  return (
    <Component id={customId || titleId} {...rest}>
      {children}
    </Component>
  );
};

export interface DialogDescriptionProps extends React.HTMLAttributes<HTMLDivElement> {
  children: React.ReactNode;
}

export const DialogDescription: React.FC<DialogDescriptionProps> = ({
  children,
  id: customId,
  ...rest
}) => {
  const { descriptionId, setHasDescription } = useDialogContext();

  useEffect(() => {
    setHasDescription(true);
    return () => setHasDescription(false);
  }, [setHasDescription]);

  return (
    <div id={customId || descriptionId} {...rest}>
      {children}
    </div>
  );
};

export interface DialogCloseProps extends React.ButtonHTMLAttributes<HTMLButtonElement> {
  children?: React.ReactNode;
}

export const DialogClose: React.FC<DialogCloseProps> = ({ children, onClick, ...rest }) => {
  const { onOpenChange } = useDialogContext();
  return (
    <button
      type="button"
      onClick={(e) => {
        onClick?.(e);
        if (!e.defaultPrevented) {
          onOpenChange(false);
        }
      }}
      {...rest}
    >
      {children}
    </button>
  );
};

export const Dialog = {
  Root: DialogRoot,
  Content: DialogContent,
  Title: DialogTitle,
  Description: DialogDescription,
  Close: DialogClose,
};
