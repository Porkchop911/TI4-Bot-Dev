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


NOISY_EVENT_TYPES = {"message_update", "tool_execution_update"}
FINAL_TEXT_LIMIT = 2_000
EVENT_TEXT_LIMIT = 500


def _short(value: Any, limit: int) -> str:
    text = value if isinstance(value, str) else json.dumps(value, ensure_ascii=False, default=str)
    text = " ".join(text.split())
    return text if len(text) <= limit else text[: limit - 1] + "…"


def _message_text(message: dict[str, Any]) -> str:
    parts = message.get("content", [])
    if not isinstance(parts, list):
        return ""
    return "\n".join(
        part.get("text", "")
        for part in parts
        if isinstance(part, dict) and part.get("type") == "text"
    )


def compact_event(event: dict[str, Any]) -> dict[str, Any] | None:
    """Reduce native Pi events to bounded operational metadata.

    Token/thinking deltas and streaming tool-output deltas are deliberately
    discarded. Full native output remains in Pi's session file; the bridge only
    exposes what an external supervisor needs to make a checkpoint decision.
    """
    event_type = event.get("type")
    if event_type in NOISY_EVENT_TYPES:
        return None
    compact: dict[str, Any] = {"type": event_type or "unknown"}
    for key in ("id", "command", "success", "toolName", "toolCallId", "isError", "reason"):
        if key in event:
            compact[key] = event[key]
    if event_type == "message_end" and isinstance(event.get("message"), dict):
        message = event["message"]
        compact["role"] = message.get("role")
        compact["stopReason"] = message.get("stopReason")
        if message.get("role") == "assistant" and message.get("stopReason") not in {"toolUse", "pending"}:
            text = _message_text(message)
            if text:
                compact["text"] = _short(text, FINAL_TEXT_LIMIT)
        if message.get("errorMessage"):
            compact["error"] = _short(message["errorMessage"], EVENT_TEXT_LIMIT)
    elif event_type == "tool_execution_start":
        compact["args"] = _short(event.get("args", {}), EVENT_TEXT_LIMIT)
    elif event_type == "tool_execution_end":
        compact["result"] = _short(event.get("result", {}), EVENT_TEXT_LIMIT)
    elif event_type == "response":
        data = event.get("data")
        if data is not None:
            compact["data"] = data
        if event.get("error"):
            compact["error"] = _short(event["error"], EVENT_TEXT_LIMIT)
    elif event_type == "controller_parse_error":
        compact["error"] = _short(event.get("error", ""), EVENT_TEXT_LIMIT)
    return compact


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
        self.managed_task: dict[str, Any] | None = None
        self.last_stop_reason: str | None = None
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
                    "mode": "low-token-bounded-supervision",
                    "started_at": self.started_at,
                },
                indent=2,
            ),
            encoding="utf-8",
        )
        threading.Thread(target=self._read_stdout, daemon=True).start()
        threading.Thread(target=self._read_stderr, daemon=True).start()
        threading.Thread(target=self._watchdog, daemon=True).start()

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
            self._update_task(event)
            reduced = compact_event(event)
            if reduced is not None:
                self._record(reduced)
        self._record({"type": "controller_pi_stdout_closed", "exitCode": self.process.poll()})

    def _git_snapshot(self) -> dict[str, Any]:
        try:
            status = subprocess.run(
                ["git", "status", "--short"],
                cwd=self.args.workspace,
                text=True,
                encoding="utf-8",
                errors="replace",
                capture_output=True,
                timeout=10,
                check=True,
                creationflags=subprocess.CREATE_NO_WINDOW,
            ).stdout.strip()
            head = subprocess.run(
                ["git", "rev-parse", "--short", "HEAD"],
                cwd=self.args.workspace,
                text=True,
                encoding="utf-8",
                errors="replace",
                capture_output=True,
                timeout=10,
                check=True,
                creationflags=subprocess.CREATE_NO_WINDOW,
            ).stdout.strip()
            return {"ok": True, "head": head, "status": _short(status, 2_000)}
        except (OSError, subprocess.SubprocessError) as exc:
            return {"ok": False, "error": _short(str(exc), EVENT_TEXT_LIMIT)}

    def _update_task(self, event: dict[str, Any]) -> None:
        event_type = event.get("type")
        with self.lock:
            task = self.managed_task
            if task is None:
                return
            task["lastEventAt"] = time.time()
            task["lastEventType"] = event_type
            if event_type == "agent_start":
                task["status"] = "running"
            elif event_type == "tool_execution_start":
                task["toolCalls"] += 1
                task["lastTool"] = {
                    "name": event.get("toolName"),
                    "args": _short(event.get("args", {}), 240),
                }
            elif event_type == "tool_execution_end" and event.get("isError"):
                task["toolErrors"] += 1
            elif event_type == "message_end" and isinstance(event.get("message"), dict):
                message = event["message"]
                self.last_stop_reason = message.get("stopReason")
                text = _message_text(message)
                if message.get("role") == "assistant" and text:
                    task["finalText"] = _short(text, FINAL_TEXT_LIMIT)
            elif event_type == "compaction_start":
                task["status"] = "compacting"
            elif event_type == "compaction_end":
                task["status"] = "resuming"
            elif event_type == "agent_end":
                task["status"] = "settling"
                task["agentEndAt"] = time.time()
            elif event_type == "agent_settled":
                task["status"] = "completed" if not task.get("abortReason") else "aborted"
                task["finishedAt"] = time.time()
                task["finalGit"] = self._git_snapshot()

    def _abort_managed_task(self, reason: str) -> None:
        with self.lock:
            task = self.managed_task
            if task is None or task.get("status") not in {
                "queued", "running", "compacting", "resuming", "settling"
            }:
                return
            task["abortReason"] = reason
            task["status"] = "aborting"
        try:
            self.send({"id": f"controller-abort-{int(time.time())}", "type": "abort"})
        finally:
            self._record({"type": "controller_task_limit", "reason": reason})

    def _watchdog(self) -> None:
        while self.process.poll() is None:
            time.sleep(1)
            now = time.time()
            reason: str | None = None
            with self.lock:
                task = self.managed_task
                if task is None:
                    continue
                status = task.get("status")
                if status == "settling" and now - task.get("agentEndAt", now) >= 3:
                    task["status"] = "completed" if not task.get("abortReason") else "aborted"
                    task["finishedAt"] = now
                    task["finalGit"] = self._git_snapshot()
                    continue
                if status not in {"queued", "running", "compacting", "resuming", "aborting"}:
                    continue
                current_git = self._git_snapshot()
                task["currentGit"] = current_git
                if current_git.get("ok") and (
                    current_git.get("status") != task["baselineGit"].get("status")
                    or current_git.get("head") != task["baselineGit"].get("head")
                ):
                    task["firstEditAt"] = task.get("firstEditAt") or now
                if status != "aborting":
                    if now >= task["deadlineAt"]:
                        reason = f"absolute timeout after {task['timeoutSeconds']} seconds"
                    elif task.get("firstEditAt") is None and now >= task["noEditDeadlineAt"]:
                        reason = f"no repository edit after {task['noEditSeconds']} seconds"
                    elif task["toolErrors"] > task["maxToolErrors"]:
                        reason = f"tool error limit exceeded ({task['toolErrors']} > {task['maxToolErrors']})"
            if reason:
                self._abort_managed_task(reason)

    def start_task(self, body: dict[str, Any]) -> dict[str, Any]:
        prompt = body.get("prompt")
        if not isinstance(prompt, str) or not prompt.strip():
            raise ValueError("prompt must be a non-empty string")
        timeout_seconds = int(body.get("timeoutSeconds", self.args.task_timeout_seconds))
        no_edit_seconds = int(body.get("noEditSeconds", self.args.no_edit_seconds))
        max_tool_errors = int(body.get("maxToolErrors", self.args.max_tool_errors))
        if not 30 <= timeout_seconds <= 3_600:
            raise ValueError("timeoutSeconds must be between 30 and 3600")
        if not 15 <= no_edit_seconds <= timeout_seconds:
            raise ValueError("noEditSeconds must be between 15 and timeoutSeconds")
        if not 0 <= max_tool_errors <= 10:
            raise ValueError("maxToolErrors must be between 0 and 10")
        with self.lock:
            if self.managed_task and self.managed_task.get("status") in {
                "queued", "running", "compacting", "resuming", "settling", "aborting"
            }:
                raise RuntimeError("a managed task is already active")
            now = time.time()
            task_id = str(body.get("id") or f"task-{int(now)}")
            baseline = self._git_snapshot()
            self.managed_task = {
                "id": task_id,
                "status": "queued",
                "startedAt": now,
                "lastEventAt": now,
                "deadlineAt": now + timeout_seconds,
                "noEditDeadlineAt": now + no_edit_seconds,
                "timeoutSeconds": timeout_seconds,
                "noEditSeconds": no_edit_seconds,
                "maxToolErrors": max_tool_errors,
                "toolCalls": 0,
                "toolErrors": 0,
                "firstEditAt": None,
                "baselineGit": baseline,
                "currentGit": baseline,
                "lastTool": None,
                "lastEventType": None,
                "finalText": "",
                "abortReason": None,
            }
        bounded_prompt = (
            "Execute exactly this one bounded task. Make the first repository edit promptly; "
            "avoid extended narration and exploratory loops. After one failed approach, use one "
            "simpler retry. Run the stated acceptance checks, report a compact result, then stop. "
            "Do not start follow-on work.\n\n" + prompt.strip()
        )
        try:
            self.send({"id": task_id, "type": "prompt", "message": bounded_prompt})
        except Exception:
            with self.lock:
                assert self.managed_task is not None
                self.managed_task["status"] = "failed-to-start"
                self.managed_task["finishedAt"] = time.time()
            raise
        return {"ok": True, "accepted": task_id, "limits": {
            "timeoutSeconds": timeout_seconds,
            "noEditSeconds": no_edit_seconds,
            "maxToolErrors": max_tool_errors,
        }}

    def summary(self) -> dict[str, Any]:
        with self.lock:
            task = dict(self.managed_task) if self.managed_task else None
            if task:
                now = time.time()
                task["elapsedSeconds"] = round(now - task["startedAt"], 1)
                task["deadlineRemainingSeconds"] = max(0, round(task["deadlineAt"] - now, 1))
                task.pop("deadlineAt", None)
                task.pop("noEditDeadlineAt", None)
            return {"ok": True, "task": task, "lastSequence": self.sequence}

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
                "managedTaskStatus": self.managed_task.get("status") if self.managed_task else None,
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
            if parsed.path == "/summary":
                self._json(200, controller.summary())
                return
            self._json(404, {"ok": False, "error": "not found"})

        def do_POST(self) -> None:  # noqa: N802
            if not self._authorized():
                self._json(403, {"ok": False, "error": "forbidden"})
                return
            if self.path not in {"/rpc", "/task"}:
                self._json(404, {"ok": False, "error": "not found"})
                return
            try:
                length = int(self.headers.get("Content-Length", "0"))
                if length <= 0 or length > controller.args.max_command_bytes:
                    raise ValueError("invalid command size")
                body = json.loads(self.rfile.read(length))
                if not isinstance(body, dict):
                    raise ValueError("request body must be an object")
                if self.path == "/task":
                    result = controller.start_task(body)
                else:
                    if not isinstance(body.get("type"), str):
                        raise ValueError("RPC command must be an object with a string type")
                    controller.send(body)
                    result = {"ok": True, "accepted": body.get("id")}
            except (ValueError, json.JSONDecodeError) as exc:
                self._json(400, {"ok": False, "error": str(exc)})
                return
            except RuntimeError as exc:
                self._json(409, {"ok": False, "error": str(exc)})
                return
            except Exception as exc:
                self._json(503, {"ok": False, "error": str(exc)})
                return
            self._json(202, result)

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
    parser.add_argument("--task-timeout-seconds", type=int, default=600)
    parser.add_argument("--no-edit-seconds", type=int, default=120)
    parser.add_argument("--max-tool-errors", type=int, default=1)
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
