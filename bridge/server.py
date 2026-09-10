# Vendored verbatim from the historical Python repository, 2026-09-09.
#
#     D:/Projects/ti4-engine  branch codex/fully-learned-policy  commit 37061c5
#     bridge/server.py
#
# Kept rather than ported. It is standard-library only, binds 127.0.0.1, imports nothing
# from the old engine, and has been exercised by real games -- so it is the one part of
# the old bridge that a rewrite could only make worse. M11-002 (transport), M11-003
# (feature negotiation), M11-004 (command ids and queue) and M11-005 (telemetry storage)
# are satisfied by this file and are struck from the Rust plan.
#
# Everything semantic lives in Rust: `ti4_bridge::hexsummary` reads the board this serves
# at /latest, and `ti4_bridge::wire` builds the commands it accepts at /queue.
#
# Do not edit this to add behaviour. If the bridge needs to do something new, the new
# thing goes in Rust and this stays a dumb pipe.

"""The local endpoint the Tabletop Simulator mod posts to.

The mod's game-data helper already POSTs the table state to a configurable host. Point
it at ``localhost`` and this is what receives it. Standard library only: the bridge
must run on a machine that is already running TTS and a game, and adding a web
framework to that is not a trade worth making.

What the mod actually does, read from ``TI4_GAME_DATA_HELPER`` rather than assumed:

- ``POST /postkey?key=localhost&timestamp=N`` -- the **fast path**, and the useful
  one. ``_uploadStreamerGameData`` needs only the game-data key, no setup opt-in, and
  ``startPeriodicUpdates`` both puts it on a timer and fires it once immediately. It
  also includes Steam names.
- ``POST /posttimestamp?timestamp=N`` -- the anonymized path. Needs ``_gamedataOptIn``
  set at setup, and runs on a fixed **ten minute** timer.
- ``/data`` -- the default when no path is given.

A correction worth recording, since I had this backwards once: the streamer call *is*
commented out inside ``triggerUploadGameData``, the debounced event-driven trigger. It
is not commented out in ``startPeriodicUpdates``, which is what ``!gamedata`` calls. So
it fires on its own timer regardless.

Three behaviours to design around rather than fight:

**Thirty seconds is the floor.** ``MIN_PERIODIC_SECONDS = 30``, so asking for a shorter
delay silently gets clamped up. ``!gamedata localhost 10`` yields a thirty second timer.

**The anonymized timer ignores that delay entirely.** It is pinned to
``DEFAULT_ANON_PERIODIC_SECONDS = 600``; only the streamer timer honours the argument.

**CRC suppression, tracked per path.** Identical payloads are dropped by the mod, so
*no news is good news*, not a lost connection. A bridge that treats silence as failure
will be wrong most of the time.

The response body is the return channel. ``WebRequest.custom`` is called with
``download = true``, so TTS does read the response -- but the mod's callback throws it
away and only checks ``is_error``. Commands are therefore prepared and returned here,
and consuming them needs a small edit to that callback in a Save As copy of the mod.
Until then this is an observer, which is stated where it is true rather than implied.
"""

from __future__ import annotations

import json
import threading
import time
from dataclasses import dataclass, field
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
from pathlib import Path
from typing import Any
from urllib.parse import parse_qs, urlparse

#: The mod's compiled-in localhost port. Not configurable from the TTS side without
#: editing the mod, so changing it here alone would silently receive nothing.
DEFAULT_PORT = 8080

#: Paths the mod posts to. ``/postkey`` is the one ``!gamedata`` drives.
ACCEPTED_PATHS = ("/posttimestamp", "/postkey", "/data")

#: Not the mod's. Lets another process queue a command into a running bridge --
#: the queue lives in memory, so without this only the process that owns the
#: listener could ever send anything, which a bot in its own process cannot.
QUEUE_PATH = "/queue"

#: How many log lines from the mod to keep.
LOG_LIMIT = 500

#: Where the executor asks for work on its own timer.
#:
#: Command delivery used to ride on the telemetry upload, which is unusable for anything
#: autonomous: the mod suppresses uploads whose CRC matches the last one, so a still
#: table sends nothing, gets no response, and can be given no orders. A bot could not act
#: unless a human moved a piece first. This channel does not care whether anything moved.
POLL_PATH = "/poll"

#: The most recent payload, as JSON, so another process can build a game from the
#: table without owning the bridge. The uploads live in memory; without this, only
#: the process holding them can read them -- the same reason QUEUE_PATH exists.
LATEST_PATH = "/latest"

#: What the executor has said back. The mod answers questions -- which planet it
#: explored, what is in a hand -- by writing to the log it returns on each poll, so
#: reading that log is how another process hears the reply.
LOG_PATH = "/log"

#: The turn colour observed by the executor's independent two-second poll.  This is a
#: signal, not a board capture: callers use it to notice that a person pressed End Turn,
#: then wait for telemetry before reading what changed on the table.
TURN_PATH = "/turn"

#: What this bridge can do, reported on the health check so a caller can tell an old
#: process from a current one *without* sending it anything.
#:
#: A bridge outlives the tool that started it and keeps the port, so a run days later
#: silently talks to whatever is already there. That is not hypothetical: a whole
#: thirty-turn game reported no command outcomes at all, because the listener predated
#: ``command_ids`` and sent every command out unnumbered -- and the executor, correctly,
#: stays quiet about a command nobody numbered. Everything looked healthy from both ends.
#:
#: Add a name here when a capability is added that a caller would be wrong to assume.
FEATURES = ("latest", "log", "poll", "command_ids", "turn_signal")


@dataclass
class Upload:
    """One payload from the mod, with when it arrived and how it was addressed."""

    path: str
    args: dict[str, str]
    payload: dict[str, Any]
    received_at: float = field(default_factory=time.time)

    @property
    def hex_summary(self) -> str | None:
        """The board string, if this payload carried one."""
        value = self.payload.get("hexSummary")
        return value if isinstance(value, str) else None

    @property
    def round(self) -> int | None:
        value = self.payload.get("round")
        return value if isinstance(value, int) else None


class Bridge:
    """Holds what the mod has said, and what we would like to say back.

    Separate from the HTTP plumbing so it can be exercised without a socket, and so the
    reconciliation layer has something to read that is not a request handler.
    """

    def __init__(self, capture_dir: Path | None = None) -> None:
        self._lock = threading.Lock()
        self._uploads: list[Upload] = []
        self._pending: list[dict[str, Any]] = []
        self._log: list[str] = []
        self._turn: str | None = None
        self._turn_received_at: float | None = None
        self._sequence = 0
        self.capture_dir = capture_dir
        if capture_dir:
            capture_dir.mkdir(parents=True, exist_ok=True)

    # -- receiving ------------------------------------------------------------------

    def receive(self, path: str, args: dict[str, str], payload: dict[str, Any]) -> Upload:
        upload = Upload(path=path, args=args, payload=payload)
        with self._lock:
            self._uploads.append(upload)
            if self.capture_dir:
                self._capture(upload)
        return upload

    def _capture(self, upload: Upload) -> None:
        """Write the payload to disk.

        Conformance for the board decoder needs samples from a real game -- it was
        written from the mod's Lua, and a misreading would round-trip happily against
        an encoder written from the same misreading. Captures are how that gets
        checked against reality.
        """
        assert self.capture_dir is not None
        name = f"{int(upload.received_at * 1000)}{upload.path.replace('/', '_')}.json"
        (self.capture_dir / name).write_text(
            json.dumps(upload.payload, indent=2), encoding="utf-8"
        )

    @property
    def uploads(self) -> tuple[Upload, ...]:
        with self._lock:
            return tuple(self._uploads)

    @property
    def latest(self) -> Upload | None:
        with self._lock:
            return self._uploads[-1] if self._uploads else None

    @property
    def latest_board(self):
        """The most recent board the mod described, decoded. ``None`` if none yet."""
        from bridge import hexsummary

        for upload in reversed(self.uploads):
            summary = upload.hex_summary
            if summary is not None:
                return hexsummary.decode(summary)
        return None

    def seconds_since_last(self) -> float | None:
        latest = self.latest
        return None if latest is None else time.time() - latest.received_at

    # -- sending back -----------------------------------------------------------------

    def queue(self, command: dict[str, Any]) -> dict[str, Any]:
        """Queue a command for the next response, and give it a number.

        The number is what makes the channel two-way. The executor reports what became of
        each command it ran, and a report is only worth having if it can be matched to the
        command it is about -- eight ``move`` commands in one batch produce eight answers,
        and "one of them was refused" is not something anybody can act on.

        Stamped here rather than by the caller because *every* sender needs it and only
        this side can promise the numbers are unique: commands arrive from another process
        over ``/queue``, which is exactly the case a caller-side counter gets wrong.

        An ``id`` the caller supplied is kept, so a command can be re-queued without
        becoming a different command.

        The stamped command comes back rather than just its number, so a caller echoing
        the result cannot show one thing and queue another.
        """
        with self._lock:
            self._sequence += 1
            if command.get("id") is None:
                command = {**command, "id": self._sequence}
            self._pending.append(command)
            return command

    def absorb_log(self, lines: list[str]) -> list[str]:
        """Take log lines the mod sent, keep them, and hand back what was new.

        Returned rather than only stored so the runner can print them as they arrive.
        Bounded, because an executor stuck in a failing loop would otherwise grow this
        without limit for as long as the bridge is up.
        """
        if not lines:
            return []
        with self._lock:
            self._log.extend(lines)
            del self._log[:-LOG_LIMIT]
        return lines

    @property
    def log(self) -> tuple[str, ...]:
        with self._lock:
            return tuple(self._log)

    def observe_turn(self, colour: str | None) -> None:
        """Remember TTS's current turn as reported by the executor poll."""
        if not colour:
            return
        with self._lock:
            self._turn = colour
            self._turn_received_at = time.time()

    @property
    def turn(self) -> tuple[str | None, float | None]:
        with self._lock:
            return self._turn, self._turn_received_at

    def take_pending(self) -> list[dict[str, Any]]:
        with self._lock:
            pending, self._pending = self._pending, []
        return pending

    @property
    def pending(self) -> tuple[dict[str, Any], ...]:
        with self._lock:
            return tuple(self._pending)


class _Handler(BaseHTTPRequestHandler):
    bridge: Bridge

    def do_POST(self) -> None:  # noqa: N802 - name fixed by BaseHTTPRequestHandler
        parsed = urlparse(self.path)

        if parsed.path == QUEUE_PATH:
            self._queue_command()
            return

        if parsed.path == POLL_PATH:
            # The poll carries the mod's own log back. Without it every failure inside
            # TTS is visible only in that game's chat, so diagnosing one means asking a
            # human to read a line out -- which is slow, lossy, and impossible for a bot
            # running unattended. A command that silently does nothing is exactly the
            # case that needs the reason, and exactly the case where nothing changes on
            # the table for the next upload to carry.
            payload = self._poll_payload()
            lines = payload.get("log")
            if isinstance(lines, list):
                self.bridge.absorb_log([str(line) for line in lines])
            turn = payload.get("turn")
            if isinstance(turn, str):
                self.bridge.observe_turn(turn)
            self._respond(200, {"commands": self.bridge.take_pending()})
            return

        if parsed.path not in ACCEPTED_PATHS:
            self._respond(404, {"error": "unknown path"})
            return

        try:
            body = self._read_body()
            payload = json.loads(body) if body else {}
        except (ValueError, OSError) as exc:
            # A malformed payload must not take the bridge down; TTS will send another.
            self._respond(400, {"error": str(exc)})
            return

        if not isinstance(payload, dict):
            self._respond(400, {"error": "payload was not an object"})
            return

        args = {k: v[0] for k, v in parse_qs(parsed.query).items()}
        self.bridge.receive(parsed.path, args, payload)
        self._respond(200, {"commands": self.bridge.take_pending()})

    def _poll_payload(self) -> dict[str, Any]:
        """The executor's poll body, or an empty object when it is malformed.

        Tolerant by design: an older executor posts ``{}`` and a broken one could post
        anything. Neither may cost the bridge a poll, because the poll is also how
        commands are delivered.
        """
        try:
            body = self._read_body()
            payload = json.loads(body) if body else {}
        except (ValueError, OSError):
            return {}
        if not isinstance(payload, dict):
            return {}
        return payload

    def _queue_command(self) -> None:
        """Accept a command for the next upload. Localhost only, by binding."""
        try:
            body = self._read_body()
            command = json.loads(body) if body else None
        except ValueError as exc:
            self._respond(400, {"error": str(exc)})
            return

        if not isinstance(command, dict) or not isinstance(command.get("action"), str):
            # The executor refuses commands with no action, so refusing here too means
            # the mistake surfaces where it was made rather than in TTS chat.
            self._respond(400, {"error": "a command needs an 'action' string"})
            return

        # The id comes back in the response, because the process that queued the command
        # is the one waiting to hear how it went and it cannot learn the number any other
        # way -- the queue is in this process's memory.
        queued = self.bridge.queue(command)
        self._respond(
            200,
            {
                "queued": queued,
                "id": queued["id"],
                "pending": len(self.bridge.pending),
            },
        )

    def do_GET(self) -> None:  # noqa: N802
        """A health check, so it is possible to tell the port is live without TTS."""
        path = urlparse(self.path).path
        if path == LOG_PATH:
            self._respond(200, {"log": list(self.bridge.log)})
            return
        if path == LATEST_PATH:
            latest = self.bridge.latest
            if latest is None:
                self._respond(404, {"error": "no upload has arrived yet"})
            else:
                self._respond(200, latest.payload)
            return
        if path == TURN_PATH:
            colour, received_at = self.bridge.turn
            if colour is None:
                self._respond(404, {"error": "no executor poll has reported a turn yet"})
            else:
                self._respond(200, {"turn": colour, "received_at": received_at})
            return
        self._respond(
            200,
            {
                "ok": True,
                "uploads": len(self.bridge.uploads),
                "pending": len(self.bridge.pending),
                "features": list(FEATURES),
            },
        )

    def _read_body(self) -> bytes:
        length = self.headers.get("Content-Length")
        if length is None:
            return b""
        return self.rfile.read(int(length))

    def _respond(self, status: int, body: dict[str, Any]) -> None:
        encoded = json.dumps(body).encode("utf-8")
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(encoded)))
        self.end_headers()
        self.wfile.write(encoded)

    def log_message(self, *args: Any) -> None:
        """Silence per-request logging; the mod posts on a timer and it is noise."""


class BridgeServer:
    """Runs the endpoint on a background thread.

    A context manager, because a half-closed listening socket on 8080 is an annoying
    thing to leave behind on a machine that will want the port again shortly.
    """

    def __init__(self, port: int = DEFAULT_PORT, capture_dir: Path | None = None) -> None:
        self.bridge = Bridge(capture_dir=capture_dir)
        handler = type("_BoundHandler", (_Handler,), {"bridge": self.bridge})
        self._server = ThreadingHTTPServer(("127.0.0.1", port), handler)
        self._thread: threading.Thread | None = None

    @property
    def port(self) -> int:
        """The bound port, which differs from the requested one when 0 was given."""
        return self._server.server_address[1]

    def start(self) -> "BridgeServer":
        self._thread = threading.Thread(target=self._server.serve_forever, daemon=True)
        self._thread.start()
        return self

    def stop(self) -> None:
        self._server.shutdown()
        self._server.server_close()
        if self._thread:
            self._thread.join(timeout=5)

    def __enter__(self) -> "BridgeServer":
        return self.start()

    def __exit__(self, *exc: object) -> None:
        self.stop()
