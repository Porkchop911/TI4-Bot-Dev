import type { ExplorationDeck, PlanetTraitsView, PlanetType } from "../../model";
import { Hint, Icon } from "../../ui";
import { useDispatch } from "../context";

/** The colour of a planet type, as on the planet cards. Never the only sign: the icon and the word say it too. */
export const TYPE_COLOR: Record<ExplorationDeck, string> = {
  cultural: "var(--tech-b)",
  hazardous: "var(--tech-r)",
  industrial: "var(--tech-g)",
  frontier: "var(--color-muted)",
};
export const TYPE_NAME: Record<PlanetType, string> = {
  cultural: "Cultural",
  hazardous: "Hazardous",
  industrial: "Industrial",
};

/**
 * What the exploration says about a planet, the same on every screen that lists planets: its
 * type, whether it is still explored, and what is attached. The type opens its deck.
 */
export function PlanetTraits({ name, traits }: { name: string; traits: PlanetTraitsView }) {
  const dispatch = useDispatch();
  if (!traits.types.length && !traits.attachments.length) {
    return null;
  }
  return (
    <span className="inline-flex flex-wrap items-baseline gap-x-2 gap-y-0.5">
      {traits.types.map((type) => (
        <button
          key={type}
          type="button"
          className="inline-flex items-baseline gap-1 rounded-sm hover:underline"
          style={{ color: TYPE_COLOR[type] }}
          aria-label={`${TYPE_NAME[type]} planet ${name} · open the ${type} exploration deck`}
          title={`Open the ${type} exploration deck`}
          onClick={(event) => {
            event.stopPropagation();
            dispatch({ type: "openReference", sheet: "exploration", deck: type });
          }}
        >
          <Icon name={type} className="size-3.5 self-center" />
          {TYPE_NAME[type]}
        </button>
      ))}
      {traits.unexplored && <span className="text-text">Unexplored</span>}
      {traits.attachments.map((item, index) => (
        <span key={index} className="inline-flex items-baseline gap-1 text-text">
          <span>{item.name}</span>
          {item.effect && <span className="text-muted tabular-nums">{item.effect}</span>}
          {item.text && (
            <span className="self-center">
              <Hint label={`Text of ${item.name}`}>{item.text}</Hint>
            </span>
          )}
        </span>
      ))}
    </span>
  );
}
