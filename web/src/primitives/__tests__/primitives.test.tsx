import { describe, it, expect, vi } from "vitest";
import { render, screen, fireEvent, act } from "@testing-library/react";
import { Dialog, Drawer, Tooltip, Popover, SvgButton, overlayStack } from "../index.ts";

describe("Accessible Primitives Suite", () => {
  describe("overlayStack (LIFO Dismissal)", () => {
    it("dismisses overlays in reverse order of registration when Escape is pressed", () => {
      const dismissFirst = vi.fn();
      const dismissSecond = vi.fn();

      const unregister1 = overlayStack.register({ id: "first", onDismiss: dismissFirst });
      const unregister2 = overlayStack.register({ id: "second", onDismiss: dismissSecond });

      fireEvent.keyDown(window, { key: "Escape" });
      expect(dismissSecond).toHaveBeenCalledTimes(1);
      expect(dismissFirst).not.toHaveBeenCalled();

      unregister2();

      fireEvent.keyDown(window, { key: "Escape" });
      expect(dismissFirst).toHaveBeenCalledTimes(1);

      unregister1();
    });

    it("only dismisses on outside pointer click when closeOnOutsideClick is true", () => {
      const dismissModal = vi.fn();
      const dismissPopover = vi.fn();
      const dummyElem = document.createElement("div");
      document.body.appendChild(dummyElem);

      // Register modal with element but closeOnOutsideClick: false (default)
      const unregister1 = overlayStack.register({
        id: "modal",
        element: dummyElem,
        closeOnOutsideClick: false,
        onDismiss: dismissModal,
      });

      // Pointer down outside
      fireEvent.pointerDown(document.body);
      expect(dismissModal).not.toHaveBeenCalled();

      // Register popover with closeOnOutsideClick: true
      const unregister2 = overlayStack.register({
        id: "popover",
        element: dummyElem,
        closeOnOutsideClick: true,
        onDismiss: dismissPopover,
      });

      fireEvent.pointerDown(document.body);
      expect(dismissPopover).toHaveBeenCalledTimes(1);

      unregister1();
      unregister2();
      document.body.removeChild(dummyElem);
    });
  });

  describe("Dialog Primitive", () => {
    it("omits aria-describedby when no Dialog.Description is mounted", () => {
      render(
        <Dialog.Root open={true} onOpenChange={vi.fn()}>
          <Dialog.Content data-testid="bare-dialog">
            <Dialog.Title>Bare Title</Dialog.Title>
            <button type="button">Action</button>
          </Dialog.Content>
        </Dialog.Root>,
      );

      const dialog = screen.getByTestId("bare-dialog");
      expect(dialog).not.toHaveAttribute("aria-describedby");
    });

    it("wires ARIA labels and descriptions, traps focus, and supports Escape dismissal", () => {
      const onOpenChange = vi.fn();

      render(
        <Dialog.Root open={true} onOpenChange={onOpenChange}>
          <Dialog.Content data-testid="test-dialog">
            <Dialog.Title as="h2">Modal Title</Dialog.Title>
            <Dialog.Description>Modal description text</Dialog.Description>
            <button type="button" data-testid="first-btn">
              First
            </button>
            <button type="button" data-testid="second-btn">
              Second
            </button>
            <Dialog.Close data-testid="close-btn">Close</Dialog.Close>
          </Dialog.Content>
        </Dialog.Root>,
      );

      const dialog = screen.getByTestId("test-dialog");
      expect(dialog).toHaveAttribute("role", "dialog");
      expect(dialog).toHaveAttribute("aria-modal", "true");

      const title = screen.getByText("Modal Title");
      const desc = screen.getByText("Modal description text");
      expect(dialog).toHaveAttribute("aria-labelledby", title.id);
      expect(dialog).toHaveAttribute("aria-describedby", desc.id);

      // Focus should be on first focusable element
      expect(screen.getByTestId("first-btn")).toHaveFocus();

      // Escape calls onOpenChange(false)
      fireEvent.keyDown(window, { key: "Escape" });
      expect(onOpenChange).toHaveBeenCalledWith(false);
    });
  });

  describe("Tooltip Primitive", () => {
    it('sets aria-describedby on trigger on pointer enter / focus and renders role="tooltip"', async () => {
      vi.useFakeTimers();

      render(
        <Tooltip content="Helper explanation" delayMs={50}>
          <button type="button" data-testid="tooltip-trigger">
            Hover Me
          </button>
        </Tooltip>,
      );

      const trigger = screen.getByTestId("tooltip-trigger");
      expect(trigger).not.toHaveAttribute("aria-describedby");
      expect(screen.queryByRole("tooltip")).toBeNull();

      // Pointer enter
      fireEvent.pointerEnter(trigger);
      act(() => {
        vi.advanceTimersByTime(50);
      });

      const tooltip = screen.getByRole("tooltip");
      expect(tooltip).toBeInTheDocument();
      expect(tooltip).toHaveTextContent("Helper explanation");
      expect(trigger).toHaveAttribute("aria-describedby", tooltip.id);

      // Pointer leave
      fireEvent.pointerLeave(trigger);
      expect(screen.queryByRole("tooltip")).toBeNull();
      expect(trigger).not.toHaveAttribute("aria-describedby");

      vi.useRealTimers();
    });

    it("dismisses tooltip when Escape is pressed", () => {
      vi.useFakeTimers();

      render(
        <Tooltip content="Helper explanation" delayMs={0}>
          <button type="button" data-testid="tooltip-trigger">
            Focus Me
          </button>
        </Tooltip>,
      );

      const trigger = screen.getByTestId("tooltip-trigger");
      fireEvent.focus(trigger);
      act(() => {
        vi.advanceTimersByTime(10);
      });

      expect(screen.getByRole("tooltip")).toBeInTheDocument();

      fireEvent.keyDown(window, { key: "Escape" });
      expect(screen.queryByRole("tooltip")).toBeNull();

      vi.useRealTimers();
    });
  });

  describe("Popover Primitive", () => {
    it("manages disclosure state, aria-haspopup, aria-expanded, and aria-controls", () => {
      render(
        <Popover
          content={
            <div>
              <p>Rich Popover Content</p>
              <button type="button" data-testid="popover-action">
                Do Action
              </button>
            </div>
          }
        >
          <button type="button" data-testid="popover-trigger">
            Open Details
          </button>
        </Popover>,
      );

      const trigger = screen.getByTestId("popover-trigger");
      expect(trigger).toHaveAttribute("aria-haspopup", "dialog");
      expect(trigger).toHaveAttribute("aria-expanded", "false");
      expect(screen.queryByTestId("accessible-popover")).toBeNull();

      fireEvent.click(trigger);
      expect(trigger).toHaveAttribute("aria-expanded", "true");
      const popover = screen.getByTestId("accessible-popover");
      expect(popover).toBeInTheDocument();
      expect(trigger).toHaveAttribute("aria-controls", popover.id);
      expect(screen.getByText("Rich Popover Content")).toBeInTheDocument();

      // Dismiss on Escape
      fireEvent.keyDown(window, { key: "Escape" });
      expect(screen.queryByTestId("accessible-popover")).toBeNull();
      expect(trigger).toHaveAttribute("aria-expanded", "false");
    });
  });

  describe("SvgButton Primitive", () => {
    it('provides accessible role="button", tabIndex, aria-label, and keyboard activation', () => {
      const onActivate = vi.fn();

      render(
        <svg viewBox="0 0 100 100">
          <SvgButton label="Activate System 18" onActivate={onActivate} data-testid="svg-btn">
            <circle cx="50" cy="50" r="20" />
          </SvgButton>
        </svg>,
      );

      const btn = screen.getByTestId("svg-btn");
      expect(btn).toHaveAttribute("role", "button");
      expect(btn).toHaveAttribute("tabindex", "0");
      expect(btn).toHaveAttribute("aria-label", "Activate System 18");

      // Click
      fireEvent.click(btn);
      expect(onActivate).toHaveBeenCalledTimes(1);

      // Keyboard activation via Enter
      fireEvent.keyDown(btn, { key: "Enter" });
      expect(onActivate).toHaveBeenCalledTimes(2);

      // Keyboard activation via Space
      fireEvent.keyDown(btn, { key: " " });
      expect(onActivate).toHaveBeenCalledTimes(3);
    });

    it("disables interactive semantics when isInteractive is false", () => {
      render(
        <svg viewBox="0 0 100 100">
          <SvgButton label="Non-interactive Hex" isInteractive={false} data-testid="svg-static">
            <polygon points="10,10 20,20 10,30" />
          </SvgButton>
        </svg>,
      );

      const staticElem = screen.getByTestId("svg-static");
      expect(staticElem).not.toHaveAttribute("role");
      expect(staticElem).not.toHaveAttribute("tabindex");
      expect(staticElem).not.toHaveAttribute("aria-label");
    });
  });

  describe("Drawer Primitive", () => {
    it("renders with modal dialog semantics, backdrop, and handles Escape", () => {
      const onClose = vi.fn();

      render(
        <Drawer open={true} onClose={onClose} title="Drawer Title" data-testid="test-drawer">
          <div>Drawer Content</div>
        </Drawer>,
      );

      const drawer = screen.getByTestId("test-drawer");
      expect(drawer).toHaveAttribute("role", "dialog");
      expect(drawer).toHaveAttribute("aria-modal", "true");
      expect(screen.getByTestId("drawer-backdrop")).toBeInTheDocument();

      fireEvent.keyDown(window, { key: "Escape" });
      expect(onClose).toHaveBeenCalledTimes(1);
    });
  });
});
