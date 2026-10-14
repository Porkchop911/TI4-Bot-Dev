import { afterEach, describe, expect, it } from "vitest";
import { render, screen } from "@testing-library/react";
import { MapPickerGallery } from "./MapPickerGallery.tsx";

const realFetch = window.fetch;
afterEach(() => {
  window.fetch = realFetch;
});

describe("MapPickerGallery", () => {
  it("renders the host sheet with fixture data", async () => {
    window.history.pushState({}, "", "/dev/map-picker?view=host&players=4");
    render(<MapPickerGallery />);
    expect(await screen.findByTestId("map-card-4pStandard")).toBeInTheDocument();
    expect(await screen.findByTestId("map-preview-board")).toBeInTheDocument();
  });

  it("renders the read-only sheet", async () => {
    window.history.pushState({}, "", "/dev/map-picker?view=readonly");
    render(<MapPickerGallery />);
    expect(await screen.findByTestId("map-preview-board")).toBeInTheDocument();
    expect(screen.queryByTestId("map-picker-choices")).toBeNull();
  });

  it("can show the error and empty states", async () => {
    window.history.pushState({}, "", "/dev/map-picker?state=error");
    const { unmount } = render(<MapPickerGallery />);
    expect(await screen.findByTestId("map-picker-error")).toBeInTheDocument();
    unmount();
    window.fetch = realFetch;
    window.history.pushState({}, "", "/dev/map-picker?state=empty");
    render(<MapPickerGallery />);
    expect(await screen.findByTestId("map-picker-empty")).toBeInTheDocument();
  });
});
