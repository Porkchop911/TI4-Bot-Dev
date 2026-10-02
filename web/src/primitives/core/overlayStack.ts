export interface OverlayEntry {
  id: string;
  onDismiss?: () => void;
  element?: HTMLElement | null;
  modal?: boolean;
  closeOnOutsideClick?: boolean;
}

class OverlayStackManager {
  private stack: OverlayEntry[] = [];
  private keydownListenerAttached = false;
  private pointerListenerAttached = false;

  private handleKeyDown = (event: KeyboardEvent) => {
    if (event.key !== "Escape") return;
    if (this.stack.length === 0) return;

    // Topmost overlay handles Escape
    const top = this.stack[this.stack.length - 1];
    if (top.onDismiss) {
      event.preventDefault();
      event.stopPropagation();
      top.onDismiss();
    }
  };

  private handlePointerDown = (event: PointerEvent) => {
    if (this.stack.length === 0) return;
    const top = this.stack[this.stack.length - 1];
    if (!top.onDismiss || !top.element || !top.closeOnOutsideClick) return;

    const target = event.target as Node | null;
    if (target && !top.element.contains(target)) {
      top.onDismiss();
    }
  };

  public register(entry: OverlayEntry): () => void {
    // Deduplicate by removing existing entry with same id
    const existingIndex = this.stack.findIndex((e) => e.id === entry.id);
    if (existingIndex !== -1) {
      this.stack.splice(existingIndex, 1);
    }
    this.stack.push(entry);

    if (!this.keydownListenerAttached && typeof window !== "undefined") {
      window.addEventListener("keydown", this.handleKeyDown, true);
      this.keydownListenerAttached = true;
    }

    if (!this.pointerListenerAttached && typeof window !== "undefined") {
      // Use pointerdown to intercept outside clicks before they trigger actions
      window.addEventListener("pointerdown", this.handlePointerDown, true);
      this.pointerListenerAttached = true;
    }

    return () => {
      this.unregister(entry.id);
    };
  }

  public unregister(id: string): void {
    const index = this.stack.findIndex((e) => e.id === id);
    if (index !== -1) {
      this.stack.splice(index, 1);
    }

    if (this.stack.length === 0 && typeof window !== "undefined") {
      window.removeEventListener("keydown", this.handleKeyDown, true);
      this.keydownListenerAttached = false;
      window.removeEventListener("pointerdown", this.handlePointerDown, true);
      this.pointerListenerAttached = false;
    }
  }

  public isTopmost(id: string): boolean {
    if (this.stack.length === 0) return false;
    return this.stack[this.stack.length - 1].id === id;
  }

  public getStack(): readonly OverlayEntry[] {
    return this.stack;
  }
}

export const overlayStack = new OverlayStackManager();
