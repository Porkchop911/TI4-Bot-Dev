"""Loopback HTTP bridge for controlling one persistent Pi RPC process.

The bridge owns Pi's stdin/stdout so another local process can safely submit RPC
commands and inspect ordered events without terminal keystroke injection.
"""

from __future__ import annotations

import argparse
from collections import deque
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer
import json
import os
from pathlib import Path
import secrets
import subprocess
import threading
import time
from typing import Any
from urllib.parse import parse_qs, urlparse


class Controller:
    def __init__(self, args: argparse.Namespace) -> None:
        self.args = args
        self.state_dir = args.state_dir.resolve()
        self.state_dir.mkdir(parents=True, exist_ok=True)
        self.token = secrets.token_urlsafe(32)
        self.started_at = time.time()
        self.sequence = 0
        self.events: deque[dict[str, Any]] = deque(maxlen=args.event_memory)
        self.lock = threading.RLock()
        self.stdin_lock = threading.Lock()
        self.events_path = self.state_dir / "events.jsonl"
        self.stderr_path = self.state_dir / "pi.stderr.log"
        (self.state_dir / "token").write_text(self.token, encoding="utf-8")

        command = [
            str(args.node),
            str(args.pi_cli),
            "--mode",
            "rpc",
            "--session",
            str(args.session),
            "--approve",
        ]
        self.process = subprocess.Popen(
            command,
            cwd=args.workspace,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            text=True,
            encoding="utf-8",
            errors="replace",
            bufsize=1,
            creationflags=subprocess.CREATE_NO_WINDOW,
        )
        (self.state_dir / "controller.json").write_text(
            json.dumps(
                {
                    "controller_pid": os.getpid(),
                    "pi_pid": self.process.pid,
                    "workspace": str(args.workspace),
                    "session": str(args.session),
                    "host": args.host,
                    "port": args.port,
                    "started_at": self.started_at,
                },
                indent=2,
            ),
            encoding="utf-8",
        )
        threading.Thread(target=self._read_stdout, daemon=True).start()
        threading.Thread(target=self._read_stderr, daemon=True).start()

    def _record(self, event: dict[str, Any]) -> None:
        with self.lock:
            self.sequence += 1
            envelope = {
                "sequence": self.sequence,
                "received_at": time.time(),
                "event": event,
            }
            self.events.append(envelope)
            with self.events_path.open("a", encoding="utf-8", newline="\n") as stream:
                stream.write(json.dumps(envelope, ensure_ascii=False) + "\n")

    def _read_stdout(self) -> None:
        assert self.process.stdout is not None
        for raw in self.process.stdout:
            line = raw[:-1] if raw.endswith("\n") else raw
            if line.endswith("\r"):
                line = line[:-1]
            if not line:
                continue
            try:
                event = json.loads(line)
            except json.JSONDecodeError as exc:
                event = {"type": "controller_parse_error", "error": str(exc), "raw": line}
            self._record(event)
        self._record({"type": "controller_pi_stdout_closed", "exitCode": self.process.poll()})

    def _read_stderr(self) -> None:
        assert self.process.stderr is not None
        with self.stderr_path.open("a", encoding="utf-8", newline="\n") as stream:
            for line in self.process.stderr:
                stream.write(line)
                stream.flush()

    def send(self, command: dict[str, Any]) -> None:
        if self.process.poll() is not None:
            raise RuntimeError(f"Pi exited with code {self.process.returncode}")
        assert self.process.stdin is not None
        payload = json.dumps(command, ensure_ascii=False)
        with self.stdin_lock:
            self.process.stdin.write(payload + "\n")
            self.process.stdin.flush()

    def health(self) -> dict[str, Any]:
        with self.lock:
            return {
                "ok": self.process.poll() is None,
                "controllerStartedAt": self.started_at,
                "piPid": self.process.pid,
                "piExitCode": self.process.poll(),
                "lastSequence": self.sequence,
                "session": str(self.args.session),
                "workspace": str(self.args.workspace),
            }

    def events_since(self, since: int) -> dict[str, Any]:
        with self.lock:
            selected = [item for item in self.events if item["sequence"] > since]
            return {
                "events": selected,
                "lastSequence": self.sequence,
                "oldestAvailableSequence": self.events[0]["sequence"] if self.events else None,
            }

    def close(self) -> None:
        if self.process.poll() is None:
            try:
                self.send({"type": "abort"})
            except Exception:
                pass
            try:
                self.process.wait(timeout=5)
            except subprocess.TimeoutExpired:
                self.process.terminate()
                try:
                    self.process.wait(timeout=5)
                except subprocess.TimeoutExpired:
                    self.process.kill()


def handler_for(controller: Controller) -> type[BaseHTTPRequestHandler]:
    class Handler(BaseHTTPRequestHandler):
        server_version = "PiRpcBridge/1"

        def _authorized(self) -> bool:
            return secrets.compare_digest(self.headers.get("X-Pi-Token", ""), controller.token)

        def _json(self, status: int, body: dict[str, Any]) -> None:
            encoded = json.dumps(body, ensure_ascii=False).encode("utf-8")
            self.send_response(status)
            self.send_header("Content-Type", "application/json; charset=utf-8")
            self.send_header("Content-Length", str(len(encoded)))
            self.end_headers()
            self.wfile.write(encoded)

        def do_GET(self) -> None:  # noqa: N802
            if not self._authorized():
                self._json(403, {"ok": False, "error": "forbidden"})
                return
            parsed = urlparse(self.path)
            if parsed.path == "/health":
                self._json(200, controller.health())
                return
            if parsed.path == "/events":
                try:
                    since = int(parse_qs(parsed.query).get("since", ["0"])[0])
                except ValueError:
                    self._json(400, {"ok": False, "error": "since must be an integer"})
                    return
                self._json(200, controller.events_since(since))
                return
            self._json(404, {"ok": False, "error": "not found"})

        def do_POST(self) -> None:  # noqa: N802
            if not self._authorized():
                self._json(403, {"ok": False, "error": "forbidden"})
                return
            if self.path != "/rpc":
                self._json(404, {"ok": False, "error": "not found"})
                return
            try:
                length = int(self.headers.get("Content-Length", "0"))
                if length <= 0 or length > controller.args.max_command_bytes:
                    raise ValueError("invalid command size")
                body = json.loads(self.rfile.read(length))
                if not isinstance(body, dict) or not isinstance(body.get("type"), str):
                    raise ValueError("RPC command must be an object with a string type")
                controller.send(body)
            except (ValueError, json.JSONDecodeError) as exc:
                self._json(400, {"ok": False, "error": str(exc)})
                return
            except Exception as exc:
                self._json(503, {"ok": False, "error": str(exc)})
                return
            self._json(202, {"ok": True, "accepted": body.get("id")})

        def log_message(self, _format: str, *_args: Any) -> None:
            return

    return Handler


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument("--workspace", type=Path, required=True)
    parser.add_argument("--session", type=Path, required=True)
    parser.add_argument("--state-dir", type=Path, required=True)
    parser.add_argument("--node", type=Path, required=True)
    parser.add_argument("--pi-cli", type=Path, required=True)
    parser.add_argument("--host", default="127.0.0.1")
    parser.add_argument("--port", type=int, default=41873)
    parser.add_argument("--event-memory", type=int, default=5000)
    parser.add_argument("--max-command-bytes", type=int, default=1_048_576)
    return parser.parse_args()


def main() -> None:
    args = parse_args()
    controller = Controller(args)
    server = ThreadingHTTPServer((args.host, args.port), handler_for(controller))
    try:
        server.serve_forever(poll_interval=0.2)
    finally:
        server.server_close()
        controller.close()


if __name__ == "__main__":
    main()
