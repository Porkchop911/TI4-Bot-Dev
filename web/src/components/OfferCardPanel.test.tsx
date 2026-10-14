import { describe, expect, it, vi } from "vitest";
import { fireEvent, render, screen } from "@testing-library/react";
import { PendingChoiceModal } from "./PendingChoiceModal.tsx";
import { describeOfferCard } from "../presentation/offerCard.ts";
import { offerCases } from "../dev/offerGalleryCases.ts";
import type { PendingChoiceDto } from "../protocol/types.ts";

const galleryChoice = (title: string): PendingChoiceDto =>
  offerCases().find((c) => c.title === title)!.choice;

describe("offer card", () => {
  it("reads the card, the facts and a caption per answer, decline last", () => {
    const view = describeOfferCard(galleryChoice("Ground hit: sustain damage"))!;
    expect(view).toMatchObject({ title: "Sustain damage", tag: "ground hit" });
    expect(view.facts.map((f) => f.label)).toEqual(["Unit", "Where"]);
    expect(view.facts[0].unit).toBe("mech");
    expect(view.facts[1].text).toBe("Jord (system 14)");
    expect(view.answers.map((a) => a.option.id)).toEqual(["sustain", "decline"]);
    expect(view.answers[0]).toMatchObject({ label: "Sustain damage", isDecline: false });
    expect(view.answers[1]).toMatchObject({ label: "Let it be destroyed", isDecline: true });
  });

  it("is not an offer without the details or the card title", () => {
    const choice = galleryChoice("Ground hit: sustain damage");
    expect(describeOfferCard({ ...choice, details: undefined })).toBeNull();
    expect(describeOfferCard({ ...choice, details: { kind: "offer" } })).toBeNull();
  });

  it("falls back to the option's own label without a caption", () => {
    const choice = galleryChoice("Ground hit: sustain damage");
    const view = describeOfferCard({
      ...choice,
      details: { kind: "offer", card: { title: "T" }, facts: [], captions: {} },
    })!;
    expect(view.answers[0].label).toBe("Use SUSTAIN DAMAGE");
  });

  it("shows a commander payment with the amounts and what each answer does", () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(
      <PendingChoiceModal choice={galleryChoice("Crimson commander: gain or convert")} onSubmit={onSubmit} />,
    );
    expect(screen.getByTestId("offer-card-panel")).toHaveTextContent("Ahk Siever");
    expect(screen.getByTestId("offer-card-facts")).toHaveTextContent("Commodities1 of 3");
    expect(screen.getByTestId("offer-card-answer-convert")).toHaveTextContent("trade goods 4 → 5");
    fireEvent.click(screen.getByTestId("offer-card-answer-gain"));
    expect(onSubmit).toHaveBeenCalledWith("gain");
  });

  it("names the technology of a research waiver and capitalises the engine's labels", () => {
    const view = describeOfferCard(galleryChoice("Research waiver: ignore prerequisites"))!;
    expect(view.facts[0].text).toBe("War Sun");
    expect(view.answers.map((a) => a.label)).toEqual([
      "Return 1 infantry to reinforcements to ignore its prerequisites",
      "Don't use a waiver",
    ]);
    const pay = describeOfferCard(galleryChoice("Research waiver: choose the payment"))!;
    expect(pay.answers.map((a) => a.option.id)).toEqual(["space|18", "planet|jord", "decline"]);
  });

  it("names the planet of the PDS question and keeps the alternative's own wording", () => {
    const view = describeOfferCard(galleryChoice("Construction: PDS or an alternative"))!;
    expect(view.facts[0].text).toBe("Jord (system 14)");
    expect(view.answers.map((a) => a.label)).toEqual([
      "Place the PDS",
      "Place 1 mech and 1 infantry on jord instead",
    ]);
  });

  it("captions every spot of a placement question and keeps the option ids", () => {
    const view = describeOfferCard(galleryChoice("Reinforcements: choose where to place"))!;
    expect(view.facts[0].unit).toBe("infantry");
    expect(view.facts[1].text).toBe("2");
    expect(view.answers.map((a) => a.option.id)).toEqual(["14|jord", "26|lodor", "18|mr", "decline"]);
    expect(view.answers[0]).toMatchObject({ label: "Jord (system 14)", hint: "Place 2 infantry here" });
  });

  it("shows the controlling seat of a coexist question by name", () => {
    const view = describeOfferCard(galleryChoice("Slumberstate Computing: coexist or fight"))!;
    expect(view.title).toBe("Slumberstate Computing");
    expect(view.facts[1]).toMatchObject({ label: "Controlled by", seat: "other_seat", text: null });
    expect(view.answers.map((a) => a.option.id)).toEqual(["fight", "coexist"]);
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(
      <PendingChoiceModal choice={galleryChoice("Slumberstate Computing: coexist or fight")} onSubmit={onSubmit} />,
    );
    fireEvent.click(screen.getByTestId("offer-card-answer-coexist"));
    expect(onSubmit).toHaveBeenCalledWith("coexist");
  });

  it("shows the owner of each candidate unit beside its hint", () => {
    const choice = galleryChoice("Ssruu: choose a unit for the copied agent");
    const view = describeOfferCard(choice)!;
    expect(view.title).toBe("Viscount Unlenn");
    expect(view.answers[1]).toMatchObject({ label: "infantry · On Jord", seat: "other_seat" });
    render(<PendingChoiceModal choice={choice} onSubmit={vi.fn()} />);
    expect(screen.getByTestId("offer-card-answer-other_seat|jord|infantry|1|0")).toHaveTextContent("Unit 2 of that kind there");
  });

  it("shows a number that changes as before → after", () => {
    const view = describeOfferCard(galleryChoice("Deepwrought commander: reduce research"))!;
    expect(view.facts[0].change).toEqual({ from: 4, to: 3, of: null });
    expect(view.answers.map((a) => a.label)).toEqual(["Reduce the cost by 1", "Pay in full"]);
  });

  it("shows the card inside the pending choice and answers with the chosen option", () => {
    const onSubmit = vi.fn().mockResolvedValue(undefined);
    render(<PendingChoiceModal choice={galleryChoice("Ground hit: sustain damage")} onSubmit={onSubmit} />);
    expect(screen.getByTestId("offer-card-panel")).toBeInTheDocument();
    expect(screen.getByTestId("offer-card-facts")).toHaveTextContent("Jord (system 14)");
    fireEvent.click(screen.getByTestId("offer-card-answer-sustain"));
    expect(onSubmit).toHaveBeenCalledWith("sustain");
  });
});
