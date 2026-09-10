"""Play `tools/play_tts.py` with the trained Rust policy in one or more seats.

    python bridge/play_with_mlp.py --seats hacan,sol --dry-run --turns 1
    python bridge/play_with_mlp.py --seats all --turns 3

Every argument except `--seats` is passed straight through to `play_tts`, so `--dry-run`,
`--hotseat`, `--play`, `--read-hands` and the rest behave exactly as they document.

**Read `--dry-run` first.** A queued command is a physical change to a live table.

How it attaches
---------------

The historical Python repository is read-only and pinned, so nothing there is edited. `play_tts`
builds its bots in one call -- `bots.seat_bots(game, seed=...)` -- and this wraps that call: the
returned bots are handed to `RustPolicy.seat`, which overrides `choose` and passes everything else
through. The driver keeps the table, the rules, the command building and the refusal reporting; the
Rust policy supplies only the judgement.

A seat whose faction the MLP has no row for is left as the Python bot, and says so. So is a seat the
policy refuses on -- the wrapped bot catches it, once per reason.
"""

from __future__ import annotations

import os
import sys
from pathlib import Path

RUST_REPO = Path(__file__).resolve().parent.parent
PYTHON_REPO = Path(os.environ.get("TI4_PYTHON_REPO", r"D:\Projects\ti4-engine"))

# Never write caches into the pinned repository.
os.environ.setdefault("PYTHONDONTWRITEBYTECODE", "1")
sys.dont_write_bytecode = True

# Only the pinned repository goes on the path. Both repositories have a `bridge` package and the
# driver needs *that* one -- bridge.server, bridge.commands, bridge.importer. So the shim is loaded
# by file path rather than by name, and the collision never arises.
sys.path.insert(0, str(PYTHON_REPO))

import importlib.util  # noqa: E402

_spec = importlib.util.spec_from_file_location(
    "ti4_rust_bot", RUST_REPO / "bridge" / "rust_bot.py"
)
assert _spec and _spec.loader
_rust_bot = importlib.util.module_from_spec(_spec)
# Registered before execution: `@dataclass` looks its own module up in `sys.modules` while the
# class body runs, and finds nothing if the module is still anonymous.
sys.modules["ti4_rust_bot"] = _rust_bot
_spec.loader.exec_module(_rust_bot)
RustPolicy = _rust_bot.RustPolicy


def _repair_researched_translation() -> None:
    """Stop a researched technology from being paid for and never delivered.

    `bridge/perform.py:_researched` looks the technology up with
    `technology.catalogue().get(alias)` and returns `[]` when that misses. The catalogue is keyed by
    *short* alias -- `gd`, `amd`, `lwd` -- so any payload naming a technology another way resolves
    to nothing. The token spend and the planet exhaust come from different translators and are
    emitted regardless, so the seat pays and receives no card, and **nothing is reported**: no
    refusal, no log line, no error.

    Observed live on 2026-09-09: Red played Technology, four seats used the secondary, four
    strategy tokens and four planets were spent, and exactly two `card` commands were generated.

    This repairs it without touching the pinned repository: on a miss, resolve by folded *name* and
    by the record's own alias field, and if that still fails, say so loudly instead of silently
    dropping the delivery. Nothing is invented -- an unresolvable technology is reported, not
    guessed at, because moving the wrong card is worse than moving none.
    """
    from bridge import commands, perform  # noqa: PLC0415
    from engine import technology  # noqa: PLC0415

    def fold(text: str) -> str:
        return "".join(c for c in str(text).lower() if c.isalnum())

    original = perform._researched

    def repaired(table, payload):
        produced = original(table, payload)
        if produced:
            return produced

        colour = table.colour(payload.get("player"))
        alias = payload.get("technology")
        if not (colour and alias):
            print(
                f"  [bridge] a research event named no technology or no colour: {payload!r}",
                file=sys.stderr,
            )
            return produced

        catalogue = technology.catalogue()
        wanted = fold(alias)
        for record in catalogue.values():
            name = getattr(record, "name", None)
            if not name:
                continue
            if fold(name) == wanted or fold(getattr(record, "alias", "")) == wanted:
                print(
                    f"  [bridge] recovered {name!r} for {colour} from alias {alias!r},"
                    " which is not a catalogue key",
                    file=sys.stderr,
                )
                return [commands.card(str(name), "area", colour, board="technology")]

        # The seat has already paid by the time this runs. Saying so is the whole point.
        print(
            f"  [bridge] {colour} PAID for technology {alias!r} and no card was delivered:"
            " it resolves to no name in the catalogue. Move it by hand.",
            file=sys.stderr,
        )
        return produced

    perform._researched = repaired
    # `@translates` captured the function into TRANSLATORS at import time, so rebinding the module
    # attribute alone would change nothing that the dispatcher actually calls.
    rebound = [
        event
        for event, handler in getattr(perform, "TRANSLATORS", {}).items()
        if handler is original
    ]
    for event in rebound:
        perform.TRANSLATORS[event] = repaired
    print(f"  [bridge] research translation repaired for {rebound}", file=sys.stderr)


#: What `bridgeDispatch` reads for each action, taken from `tts/bridge_executor.lua`.
#:
#: A command missing one of these is refused *by the table*, and the reason goes into a TTS chat
#: window rather than back to whoever built it. Catching it here costs nothing and turns a silent
#: mid-turn stall into a named error before anything physical happens.
REQUIRED_FIELDS: dict[str, tuple[str, ...]] = {
    "ping": (),
    "activate": ("color", "tile"),
    "move": ("color", "from", "to", "units"),
    "land": ("color", "tile", "planet", "units"),
    "claim": ("color", "tile", "planet"),
    "control": ("color", "tile", "planet"),
    "card": ("color", "name", "to"),
    "token": ("color", "kind", "count"),
    "gain_token": ("color", "pool"),
    "spend_token": ("color", "pool"),
    "return_token": ("color", "tile"),
    # `objective` is deliberately optional: a custodians point, an agenda or Imperial's Mecatol
    # point has no card behind it, and then only the scoreboard moves. Requiring it here produced
    # a false positive on the first run -- the Lua reads the field, which is not the same as
    # needing it.
    "score": ("color", "points"),
    "speaker": ("color",),
    "end_turn": ("to",),
    # Only `name` is required: `bridgeFlip` fails on a missing name and treats colour as a
    # disambiguator. It is optional but load-bearing -- the Lua notes that four objects on a real
    # table are called Construction and three are called Warfare, so without a colour the executor
    # may flip a copy from a set nobody is playing with. Legal, and worth seeing.
    "flip": ("name",),
    "place": ("color", "tile", "planet", "units"),
    "remove": ("color", "tile", "planet", "units"),
    "deal": ("color", "deck", "count"),
    "reveal_objective": ("name",),
    "report_zone": ("color",),
}


#: Commands built this run that the executor could not have read.
MALFORMED: list[str] = []

#: Shapes that are legal but worth a human's eye.
NOTES: list[str] = []


def _check(command: dict) -> bool:
    """Whether the executor could read this command. Reports the first time each shape is seen.

    Checked where commands are *built*, not where they are sent, so `--dry-run` exercises it too --
    a validator that only runs on the live path is one nobody tests until it matters.

    An action this table does not know is not an error: the mod is developed independently and
    gains actions, and refusing an unknown one would break the bridge on the next mod update.
    """
    action = str(command.get("action"))
    required = REQUIRED_FIELDS.get(action)
    if required is None:
        return True
    missing = [field for field in required if command.get(field) is None]
    if not missing:
        # Legitimate, but worth seeing: no card is marked, so the table records the points and
        # not which objective earned them.
        if action == "score" and command.get("objective") is None:
            note = f"score with no objective (cardless point): {command!r}"
            if note not in NOTES:
                NOTES.append(note)
        if action == "flip" and command.get("color") is None:
            note = (
                f"flip with no colour -- the executor picks by name alone and duplicate"
                f" cards exist on a real table: {command!r}"
            )
            if note not in NOTES:
                NOTES.append(note)
        return True
    problem = f"{action} is missing {missing}: {command!r}"
    if problem not in MALFORMED:
        MALFORMED.append(problem)
        print(f"  [bridge] MALFORMED -- {problem}", file=sys.stderr)
    return False


#: Event -> how many times its translator produced no command this run.
EMPTY_TRANSLATIONS: dict[str, int] = {}


def _audit_translations() -> dict[str, int]:
    """Report every engine event that produced no table command.

    `bridge/perform.py` has 56 `return []` sites. Some are honest -- the event needs nothing on the
    table. Others are lookup failures: a planet that did not locate, a tile that did not resolve, a
    colour that did not map. Both look identical from outside, and the second kind is how a seat
    pays for a technology and receives nothing (see `_repair_researched_translation`).

    Rather than guess which of the 56 matter, this counts them. An event that translated to nothing
    is not necessarily wrong, but a *list* of them after a turn is the difference between a bridge
    you can trust and one that is quietly dropping a third of what it is told.

    Reporting only. Nothing is invented and no command is synthesised.
    """
    from bridge import perform  # noqa: PLC0415

    def watched(event: str, translator):
        def wrapper(table, payload):
            produced = translator(table, payload)
            if not produced:
                EMPTY_TRANSLATIONS[event] = EMPTY_TRANSLATIONS.get(event, 0) + 1
            for command in produced or []:
                if isinstance(command, dict):
                    _check(command)
            return produced

        return wrapper

    for event, translator in list(perform.TRANSLATORS.items()):
        perform.TRANSLATORS[event] = watched(event, translator)
    return EMPTY_TRANSLATIONS


def _report_empty_translations() -> None:
    """Say which events reached the table as nothing. Silence here is the good outcome."""
    if not EMPTY_TRANSLATIONS:
        print("  [bridge] every translated event produced at least one command")
        return
    total = sum(EMPTY_TRANSLATIONS.values())
    print(f"  [bridge] {total} engine event(s) produced NO table command:")
    for event, count in sorted(EMPTY_TRANSLATIONS.items(), key=lambda kv: -kv[1]):
        print(f"             {event} x{count}")
    print("             Some of these are legitimately nothing to do. Any that moved a piece,")
    print("             spent a resource or gained a card on the engine's side did not reach")
    print("             the table, and the two boards have diverged.")


def main() -> int:
    arguments = sys.argv[1:]

    def take(flag: str, default: str) -> str:
        if flag in arguments:
            index = arguments.index(flag)
            value = arguments[index + 1]
            del arguments[index : index + 2]
            return value
        return default

    wanted = take("--seats", "all")
    bundle = take("--bundle", _rust_bot.DEFAULT_BUNDLE)
    temperature = float(take("--temperature", "0.001"))
    sys.argv = [sys.argv[0], *arguments]

    from engine import bots  # noqa: PLC0415 - after sys.path is set
    from tools import play_tts  # noqa: PLC0415

    _repair_researched_translation()
    _audit_translations()

    chosen = None if wanted == "all" else {s.strip() for s in wanted.split(",") if s.strip()}
    # Printed, not assumed. Which weights are playing is the first thing anyone asks of a game
    # afterwards, and a default that is only visible in source is a default nobody checked.
    print(f"  [rust policy] checkpoint {bundle} at temperature {temperature}")
    policy = RustPolicy(bundle=bundle, temperature=temperature, repo=RUST_REPO)
    original = bots.seat_bots
    wrappers: dict[str, object] = {}

    def seat_bots_with_policy(*args, **kwargs):
        seated = original(*args, **kwargs)
        # `seat_bots` also registers each bot into `game.table`, and that -- not the dict it
        # returns -- is what the engine asks at a decision. Replacing only the dict entry wraps
        # a handle nobody consults, which is exactly what it did on the first attempt: six seats
        # reported wrapped, zero decisions answered.
        game = kwargs.get("game") or (args[0] if args else None)
        for faction, bot in list(seated.items()):
            if chosen is not None and faction not in chosen:
                continue
            wrapper = policy.seat(bot)
            seated[faction] = wrapper
            wrappers[faction] = wrapper
            if game is not None:
                game.table.seat(faction, wrapper)
        names = ", ".join(sorted(wrappers)) or "none"
        print(f"  [rust policy] seats answered by the MLP: {names}")
        return seated

    bots.seat_bots = seat_bots_with_policy
    play_tts.bots.seat_bots = seat_bots_with_policy

    try:
        code = play_tts.main()
    finally:
        bots.seat_bots = original
        answered = sum(getattr(w, "answered", 0) for w in wrappers.values())
        fell_back = sum(getattr(w, "fell_back", 0) for w in wrappers.values())
        print(f"\n  [rust policy] answered {answered} decision(s), fell back {fell_back}")
        print(f"  [rust policy] {policy.timing()}")
        if MALFORMED:
            print(f"  [bridge] {len(MALFORMED)} command shape(s) the executor cannot read:")
            for problem in MALFORMED:
                print(f"             {problem}")
        else:
            print("  [bridge] every built command carries the fields the executor reads")
        for note in NOTES:
            print(f"  [bridge] note: {note}")
        _report_empty_translations()
        policy.close()
    return code


if __name__ == "__main__":
    raise SystemExit(main())
