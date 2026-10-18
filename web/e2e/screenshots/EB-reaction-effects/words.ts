import type { Phrase, Subject } from "@ti4/shared/wire/index.ts";
import type { ActiveEffectDto, EffectExpiryDto, EffectPreviewDto, TriggerUnitsDto } from "../../../src/protocol/types";

// The engine words each effect (`EffectPreview.phrase`, `ActiveEffectView.title`/`summary`/
// `expiry`, in `reaction_preview.rs` and `view/effects.rs`). The scenes here are synthetic, so this
// gives their facts the same words, with the seats, systems, planets, units and cards as subjects
// that the page names.

/** The facts of a preview, as the scenes write them; `worded` adds the phrase. */
export type PreviewFacts = Omit<EffectPreviewDto, "option" | "card" | "phrase">;
/** The facts of an active effect, as the scenes write them; `wordedEffect` adds the words. */
export type EffectFacts = Omit<ActiveEffectDto, "title" | "summary" | "expiry">;

/** Collects the subjects of one phrase: each call gives its `{key}`. */
function phraseBuilder() {
  const refs: Record<string, Subject> = {};
  const ref = (subject: Subject) => {
    const key = `r${Object.keys(refs).length}`;
    refs[key] = subject;
    return `{${key}}`;
  };
  return { ref, done: (text: string): Phrase => ({ text, refs }) };
}

const humanize = (id: string) => id.replace(/_/g, " ").replace(/\b\w/g, (char) => char.toUpperCase());

function plural(count: number, one: string, many = `${one}s`): string {
  return `${count} ${count === 1 ? one : many}`;
}

/** What playing the card would do right now, as one or two sentences. */
export function worded(preview: PreviewFacts): PreviewFacts & { phrase: Phrase } {
  const { ref, done } = phraseBuilder();
  const seatName = (id: string) => ref({ kind: "seat", player: id });
  const possessive = (id: string | undefined) => (id ? ref({ kind: "seat_possessive", player: id }) : "a player's");
  const seatsPhrase = (seats: readonly string[] | undefined) => {
    const names = (seats ?? []).map(seatName);
    if (names.length <= 1) return names[0] ?? "an opponent";
    return `${names.slice(0, -1).join(", ")} and ${names[names.length - 1]}`;
  };
  const system = (id: string | undefined) => (id ? ref({ kind: "system", system: id }) : null);
  const planet = (id: string) => ref({ kind: "planet", planet: id });
  const unitName = (unit: TriggerUnitsDto) => ref({ kind: "unit", unit_type: unit.unit_type });
  const unitList = (units: readonly TriggerUnitsDto[]) =>
    units.map(({ unit_type, count }) => ref({ kind: "units", unit_type, count })).join(", ");
  const cardName = (alias: string | undefined) => (alias ? ref({ kind: "action_card", card: alias }) : null);
  const strategyName = (id: string) => ref({ kind: "strategy_card", card: id });
  const where = preview.system ? ` in ${system(preview.system)}` : "";
  const unit = preview.units?.[0];
  const seat = preview.seats?.[0];
  const amount = preview.amount ?? 0;
  const nothing = (text: string) => `${text} Playing it now would change nothing.`;
  const text = ((): string => {
    switch (preview.effect) {
      case "cancel_card": {
        const card = cardName(preview.refs?.[0]);
        return `Cancels ${possessive(seat)} ${card ?? "action card"}. The card stays spent and its effect does not happen.`;
      }
      case "destroy_unit": {
        const target = `${possessive(unit?.owner)} ${unit ? unitName(unit) : "ship"}${where}`;
        return preview.fizzles === "immune"
          ? nothing(`${target} cannot be destroyed by Direct Hit.`)
          : `Destroys ${target}, the ship that just sustained damage.`;
      }
      case "cancel_hits": {
        const of = preview.of;
        const cancelled = of === undefined ? amount : Math.min(amount, of);
        const from = seat ? ` from ${possessive(seat)} space cannon fire` : "";
        return of === undefined
          ? `Cancels up to ${plural(amount, "hit")}${from}.`
          : `Cancels ${cancelled} of the ${plural(of, "hit")}${from} about to be assigned${where}${
              cancelled < of ? `, leaving ${of - cancelled}` : ", so none are assigned"
            }.`;
      }
      case "produce_hits":
        return `Produces ${plural(amount, "hit")} against ${possessive(seat)} ships${where}.`;
      case "take_commodities": {
        if (preview.fizzles) return nothing(`${seatsPhrase(preview.seats)} hold no commodities.`);
        const from = preview.seats?.length ? ` (${seatsPhrase(preview.seats)})` : "";
        return `You would gain ${plural(amount, "commodity", "commodities")}${from}, as trade goods.`;
      }
      case "return_committed_unit": {
        const at = preview.planets?.[0];
        return `Returns ${possessive(unit?.owner)} ${unit ? unitName(unit) : "unit"} from ${
          at ? planet(at) : "the planet"
        } to the space area${where}; it does not land.`;
      }
      case "replace_structures": {
        const at = preview.planets?.[0] ? planet(preview.planets[0]) : "that planet";
        if (preview.fizzles) return nothing(`You have no PDS or space dock on ${at}.`);
        return `Replaces your ${unitList(preview.units ?? [])} on ${at} with matching units from your reinforcements.`;
      }
      case "discard_agenda": {
        const agenda = preview.refs?.[0] ? ref({ kind: "agenda", card: preview.refs[0] }) : "the agenda";
        return `Discards ${agenda} with no effect and no replacement, then exhausts ${plural(amount, "ready planet")} of yours.`;
      }
      case "return_command_token":
        return `Returns your command token in ${system(preview.system) ?? "that system"} to your tactic pool.`;
      case "force_retreat":
        return `${seatsPhrase(preview.seats)} must announce a retreat${where}, if able.`;
      case "bar_retreat":
        return `${seatsPhrase(preview.seats)} cannot retreat${where} this combat round.`;
      case "barrage_hits_all_ships":
        return "The hits of your anti-fighter barrage roll are produced against all ships, not just fighters.";
      case "repair_units":
        return preview.fizzles
          ? nothing(`None of your units${where} has sustained damage.`)
          : `Repairs your ${unitList(preview.units ?? [])}${where}.`;
      case "disable_pds":
        return preview.fizzles
          ? nothing(`No opposing PDS${where}.`)
          : `${possessive(seat)} ${unitList(preview.units ?? [])}${where} lose Planetary Shield and Space Cannon during this invasion.`;
      case "bombardment_penalty": {
        const planets = (preview.planets ?? []).map(planet);
        return `Applies ${amount} to every bombardment roll against ${
          planets.length ? planets.join(", ") : "your planets"
        } during this invasion.`;
      }
      case "grant_bombardment":
        return preview.fizzles
          ? nothing(`You have no non-fighter ship without bombardment${where}.`)
          : `Your ${unitList(preview.units ?? [])}${where} gain Bombardment ${amount || 6} until the end of this invasion.`;
      case "fighter_bonus":
        return preview.fizzles
          ? nothing(`You have no fighters${where}.`)
          : `+${amount || 2} to each of your ${unitList(preview.units ?? [])}' combat rolls this combat round.`;
      case "gain_fleet_tokens":
        return `You gain ${plural(amount || 2, "command token")} in your fleet pool (${
          seat
            ? `${seatsPhrase(preview.seats)} ${preview.seats!.length > 1 ? "have" : "has"} ships${where}`
            : `activated${where}`
        }).`;
      case "replenish_commodities":
        return amount === 0
          ? nothing("Your commodities are already full.")
          : `Replenishes your commodities: +${amount}${preview.of ? ` (to ${preview.of})` : ""}.`;
      case "vote_last":
        return "You vote last on this agenda, after the speaker.";
      case "extra_votes":
        return `Casts ${amount || 5} additional votes for the outcome you just chose.`;
      case "keep_strategy_cards": {
        const names = (preview.refs ?? []).map(strategyName);
        return `You keep ${names.length ? names.join(", ") : "your strategy cards"} instead of returning ${names.length === 1 ? "it" : "them"}, and you do not choose a strategy card next strategy phase.`;
      }
      case "cancel_strategic_action": {
        const card = preview.refs?.[0] ? strategyName(preview.refs[0]) : "strategy card";
        return `Cancels ${possessive(seat)} ${card} strategic action: it is not resolved, the card is not exhausted and their turn ends.`;
      }
      case "become_elected":
        return `You are the elected player instead${seat ? ` of ${seatName(seat)}` : ""}, and the agenda's effect applies to you.`;
      case "take_discarded_card": {
        const card = cardName(preview.refs?.[0]) ?? "that card";
        return preview.fizzles
          ? nothing(`${card} has no component action.`)
          : `Takes ${card} from the discard pile into your hand.`;
      }
      case "reroll_all_dice":
        return `${seat ? seatName(seat) : "That player"} must reroll all of their dice for that roll.`;
      case "replace_agenda":
        return "Discards the revealed agenda and reveals the next one from the deck; players vote on that one instead.";
      case "combat_roll_bonus":
        return `+${amount || 1} to the result of each of your combat rolls during this combat round.`;
      case "move_bonus":
        return `+${amount || 1} to the move value of each of your ships during this tactical action.`;
      case "wormholes_adjacent":
        return "Systems with alpha and beta wormholes count as adjacent to each other during this tactical action.";
      case "ignore_anomalies":
        return "You ignore the effects of anomalies while moving during this tactical action.";
      case "no_space_cannon":
        return `Other players cannot use Space Cannon against your ships during the movement step${where}.`;
      case "production_boost":
        return `+${amount || 4} to the total PRODUCTION value of your units and -1 to the combined cost of the units you produce.`;
      case "extra_action":
        return "After this action you perform one additional action before the turn passes.";
      case "puppet_action":
        return "You perform one more action even though you have passed.";
      case "duress":
        return `If ${seat ? possessive(seat) : "that player's"} next action is not a strategic action, they discard all action cards, give you all trade goods and show you their secret objectives.`;
      case "skip_next_turn":
        return "The next player's turn is skipped.";
      case "black_market": {
        const partner = seat ? ` with ${seatName(seat)}` : "";
        return `Your transaction${partner} may also include relic fragments, action cards and unscored secret objectives. It cannot be canceled.`;
      }
      default:
        throw new Error(`no words for the preview ${preview.effect}`);
    }
  })();
  return { ...preview, phrase: done(text) };
}

const EXPIRY_TEXT: Record<EffectExpiryDto, string> = {
  combat_round: "until the end of this combat round",
  tactical_action: "until the end of this tactical action",
  invasion: "until the end of this invasion",
  agenda: "until this agenda is resolved",
  next_action: "until their next action",
  negotiation: "until this negotiation closes",
  next_strategy_phase: "until the next strategy phase has passed",
};

const POOL_TITLES: Record<string, string> = {
  cancel_hits: "Shields Holding / Maneuvering Jets",
};

/** The words of one lingering effect: the card, what it does and until when. */
export function wordedEffect(effect: EffectFacts): ActiveEffectDto {
  const titled = phraseBuilder();
  const title = titled.done(
    effect.card
      ? titled.ref({ kind: "action_card", card: effect.card })
      : (POOL_TITLES[effect.effect] ?? humanize(effect.effect)),
  );
  const { ref, done } = phraseBuilder();
  const amount = effect.amount ?? 0;
  const copies = effect.copies && effect.copies > 1 ? ` (${effect.copies} cards)` : "";
  const other = effect.other ? ref({ kind: "seat", player: effect.other }) : "the opponent";
  const summary = ((): string => {
    switch (effect.effect) {
      case "combat_roll_bonus":
        return `+${amount || 1} to all combat rolls`;
      case "fighter_bonus":
        return `+${amount || 2} to fighter combat rolls${copies}`;
      case "force_retreat":
        return `${other} must announce a retreat, if able`;
      case "barrage_hits_all_ships":
        return "Anti-fighter barrage hits all ships";
      case "bar_retreat":
        return "Cannot retreat";
      case "cancel_hits":
        return `Cancels the next ${plural(amount, "hit")}`;
      case "move_bonus":
        return `+${amount || 1} move for all ships`;
      case "ignore_anomalies":
        return "Ignores anomalies while moving";
      case "no_space_cannon":
        return `Others cannot use Space Cannon against these ships${copies}`;
      case "wormholes_adjacent":
        return "Alpha and beta wormholes are adjacent";
      case "production_boost":
        return `+${amount || 4} production, -1 cost${copies}`;
      case "bombardment_penalty":
        return `${amount || -4} to bombardment against their planets${copies}`;
      case "grant_bombardment":
        return `Non-fighter ships gain Bombardment ${amount || 6}`;
      case "disable_pds":
        return "Opposing PDS lose Planetary Shield and Space Cannon";
      case "vote_last":
        return "Votes last on this agenda";
      case "extra_votes":
        return `+${amount} votes on this outcome`;
      case "keep_strategy_cards":
        return "Keeps strategy cards and skips the next pick";
      case "duress":
        return `Marked by ${other}: a non-strategic action costs cards, trade goods and secrets`;
      case "black_market":
        return "Relic fragments, action cards and secrets can be traded";
      default:
        throw new Error(`no words for the effect ${effect.effect}`);
    }
  })();
  return { ...effect, title, summary: done(summary), expiry: EXPIRY_TEXT[effect.until] ?? "" } as ActiveEffectDto;
}
