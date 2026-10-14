import { fireEvent, render, screen } from "@testing-library/react";
import { afterEach, expect, it, vi } from "vitest";
import { Hint, Peek } from "./overlays";

const button = () => screen.getByRole("button", { name: "Rule" });
const note = () => screen.queryByRole("note");

afterEach(() => vi.restoreAllMocks());

it("shows the text on hover and hides it when the pointer leaves", () => {
  render(<Hint label="Rule">Gain 3 command tokens.</Hint>);
  expect(note()).toBeNull();
  expect(button()).toHaveAttribute("aria-expanded", "false");
  fireEvent.mouseEnter(button());
  expect(note()).toHaveTextContent("Gain 3 command tokens.");
  expect(button()).toHaveAttribute("aria-expanded", "true");
  fireEvent.mouseLeave(button());
  expect(note()).toBeNull();
});

it("shows the text on focus and hides it on blur", () => {
  render(<Hint label="Rule">Text</Hint>);
  fireEvent.focus(button());
  expect(note()).toBeInTheDocument();
  fireEvent.blur(button());
  expect(note()).toBeNull();
});

it("keeps the text open after a click, until a second click", () => {
  render(<Hint label="Rule">Text</Hint>);
  fireEvent.mouseEnter(button());
  fireEvent.click(button());
  fireEvent.mouseLeave(button());
  fireEvent.blur(button());
  expect(note()).toBeInTheDocument();
  fireEvent.click(button());
  expect(note()).toBeNull();
});

it("closes a kept text with Esc and with a press outside, not with a press inside", () => {
  render(
    <>
      <Hint label="Rule">Text</Hint>
      <p>Outside</p>
    </>,
  );
  fireEvent.click(button());
  fireEvent.pointerDown(note()!);
  expect(note()).toBeInTheDocument();
  fireEvent.pointerDown(screen.getByText("Outside"));
  expect(note()).toBeNull();
  fireEvent.click(button());
  fireEvent.keyDown(document, { key: "Escape" });
  expect(note()).toBeNull();
});

it("closes a text that is only shown with Esc, and leaves it at a press outside", () => {
  render(
    <>
      <Hint label="Rule">Text</Hint>
      <p>Outside</p>
    </>,
  );
  fireEvent.mouseEnter(button());
  fireEvent.pointerDown(screen.getByText("Outside"));
  expect(note()).toBeInTheDocument();
  fireEvent.keyDown(document, { key: "Escape" });
  expect(note()).toBeNull();
});

it("tells the trigger when the text is kept open", () => {
  render(
    <Peek label="Rule" trigger="Name" buttonClassName={(pinned) => (pinned ? "pinned" : "free")}>
      Text
    </Peek>,
  );
  expect(button()).toHaveTextContent("Name");
  fireEvent.mouseEnter(button());
  expect(button()).toHaveClass("free");
  fireEvent.click(button());
  expect(button()).toHaveClass("pinned");
});

// jsdom has no layout: the place of the trigger is given, in a window of 1024×768.
const at = (top: number) =>
  vi
    .spyOn(HTMLElement.prototype, "getBoundingClientRect")
    .mockReturnValue(new DOMRect(500, top, 100, 20));

it("opens below the trigger, as wide as asked, and inside the window", () => {
  at(100);
  render(
    <Peek label="Rule" trigger="Name" buttonClassName={() => ""} width={264}>
      Text
    </Peek>,
  );
  fireEvent.mouseEnter(button());
  expect(note()).toHaveStyle({ top: "120px", left: "488px", width: "264px" });
});

it("opens above the trigger when there is no room below", () => {
  at(700);
  render(<Hint label="Rule">Text</Hint>);
  fireEvent.mouseEnter(button());
  // The lower edge of the text is at the upper edge of the trigger.
  expect(note()).toHaveStyle({ bottom: "68px" });
  expect(note()!.style.top).toBe("");
});

it("a tall text opens on the side with more room, and scrolls inside that room", () => {
  at(500);
  render(
    <Peek label="Rule" trigger="Name" buttonClassName={() => ""} tall>
      Text
    </Peek>,
  );
  fireEvent.mouseEnter(button());
  // 500 above the trigger and 248 below it: above, with a margin to the edge of the window.
  expect(note()).toHaveStyle({ bottom: "268px" });
  expect(note()!.firstElementChild).toHaveStyle({ maxHeight: "486px" });
});
