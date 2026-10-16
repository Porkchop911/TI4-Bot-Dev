import { describe, it, expect } from "vitest";
import { render, screen } from "@testing-library/react";
import { NotOfferedNote, notOffered } from "./NotOfferedNote.tsx";
import type { PendingChoiceDto } from "../protocol/types.ts";

const choice = (details?: Record<string, unknown>) =>
  ({ actor: "s", nonce: "n", prompt: "p", options: [], details }) as unknown as PendingChoiceDto;

describe("NotOfferedNote", () => {
  it("lists each missing unit with its reason", () => {
    render(
      <NotOfferedNote
        choice={choice({ not_offered: [{ unit: "destroyer", reason: "the fleet supply in this system is full" }] })}
      />,
    );
    expect(screen.getByTestId("not-offered-destroyer")).toHaveTextContent("destroyer: the fleet supply in this system is full");
  });
  it("renders nothing without details and ignores malformed entries", () => {
    const { container } = render(<NotOfferedNote choice={choice()} />);
    expect(container).toBeEmptyDOMElement();
    expect(notOffered(choice({ not_offered: [{ unit: 1 }, null, "x"] }))).toEqual([]);
  });
});
