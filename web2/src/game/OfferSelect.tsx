import type { OfferMode, OfferView } from "../model";
import { Select } from "../ui/controls";
import { useDispatch } from "./context";

/**
 * When a reaction is offered to the viewer. The same control in the row of a reaction window and
 * on the card in a sheet: the window is where a reaction is muted, the sheet where it is set back.
 */
export function OfferSelect({
  offer,
  compact = false,
}: {
  offer: OfferView;
  /** In a row of a window: a small list in the line of the name, with no frame until hover. */
  compact?: boolean;
}) {
  const dispatch = useDispatch();
  return (
    <Select
      className={
        compact
          ? "border-transparent bg-transparent px-1 py-0.5 text-muted hover:border-line hover:text-text focus-visible:border-line"
          : undefined
      }
      aria-label={`Offer ${offer.name}`}
      title="A window that offers only what you muted is passed without a question"
      value={offer.mode}
      onChange={(event) =>
        dispatch({
          type: "setOffer",
          key: offer.key,
          name: offer.name,
          mode: event.target.value as OfferMode,
        })
      }
    >
      <option value="always">Offered</option>
      {(offer.ownTurn || offer.mode === "ownturn") && (
        <option value="ownturn">Only on my turns</option>
      )}
      <option value="never">Never</option>
    </Select>
  );
}
