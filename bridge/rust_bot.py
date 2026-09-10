"""Let the trained Rust policy answer a Python bot's choices.

    from bridge.rust_bot import RustPolicy

    policy = RustPolicy(bundle="out/checkpoints/stage2-mlp-shaped-resumed/checkpoint-241428")
    seated["hacan"] = policy.seat(seated["hacan"])   # wraps, keeps the ScoredBot as fallback

`tools/play_tts.py` in the historical repository already reads the table, seats a game from it,
offers each bot its legal choices and carries the answer back to TTS. The one thing it cannot do is
ask the *trained* checkpoint, because the MLP scores a decision against an observation built by the
Rust engine and refuses to answer without one.

So this is the seam, and only the seam: Python keeps the table, the rules and the legality; Rust
supplies the judgement.

How it holds together
---------------------

`engine.choice.Option` and `ti4_engine::ChoiceOption` are direct ports -- ``id``, ``kind``,
``label``, ``payload`` on both sides -- so a choice crosses as JSON with no translation. The answer
comes back as an option id, and the Rust side validates it was on offer before replying, so this can
never be handed something the driver did not present. It is checked again here anyway, because a
trust boundary that is only enforced on one side is not one.

What it does when it cannot answer
----------------------------------

Falls back to the wrapped bot and says so, once per reason. A policy that cannot score a decision is
a fact worth seeing; guessing instead is the failure this whole design exists to avoid. Pass
``fallback=None`` to make a refusal fatal instead, which is what a scored evaluation run wants.
"""

from __future__ import annotations

import json
import subprocess
import sys
import time
from dataclasses import dataclass
from pathlib import Path
from typing import Any


DEFAULT_BUNDLE = "out/checkpoints/stage2-mlp-shaped-resumed/checkpoint-241428"


class PolicyRefused(RuntimeError):
    """The Rust policy would not answer this choice."""


@dataclass
class Reply:
    """One answer, with the measurement that says whether it meant anything."""

    id: str
    label: str
    assigned: int
    oov: int

    @property
    def known(self) -> float:
        """Share of features the trained vocabulary recognised.

        A decision scored mostly from out-of-vocabulary columns is a coin flip wearing a policy's
        coat, so this is worth watching rather than averaging away.
        """
        total = self.assigned + self.oov
        return 1.0 if total == 0 else self.assigned / total


class RustPolicy:
    """A long-lived `mlp_serve` process, asked one choice at a time."""

    def __init__(
        self,
        bundle: str = DEFAULT_BUNDLE,
        captures: str = "out/bridge-captures",
        temperature: float = 0.001,
        repo: str | Path = ".",
        binary: str | None = None,
    ) -> None:
        self.repo = Path(repo).resolve()
        # The example is expected to be built already: `cargo run` inside a decision would put a
        # compile in the middle of somebody's turn.
        self.binary = binary or str(
            self.repo / "target" / "release" / "examples" / "mlp_serve.exe"
        )
        if not Path(self.binary).exists():
            raise FileNotFoundError(
                f"{self.binary} is not built. Run:\n"
                f"  cargo build --release -p ti4-mlp --example mlp_serve"
            )
        self._process = subprocess.Popen(
            [
                self.binary,
                "--bundle",
                bundle,
                "--captures",
                captures,
                "--temperature",
                repr(temperature),
            ],
            cwd=self.repo,
            stdin=subprocess.PIPE,
            stdout=subprocess.PIPE,
            stderr=None,  # straight to the terminal, so a refusal is visible where it happens
            text=True,
            bufsize=1,
        )
        self._said: set[str] = set()
        #: Seconds each answered decision took, round trip through the service.
        self.latencies: list[float] = []

    def ask(self, choice: Any) -> Reply:
        """Answer one choice, or raise :class:`PolicyRefused`."""
        if self._process.poll() is not None:
            raise PolicyRefused("the policy process has exited")

        request = {
            "choice": {
                "player": str(choice.player),
                "prompt": choice.prompt,
                "options": [
                    {
                        "id": option.id,
                        "kind": option.kind,
                        "label": option.label,
                        "payload": option.payload,
                    }
                    for option in choice.options
                ],
            }
        }
        assert self._process.stdin and self._process.stdout
        started = time.perf_counter()
        self._process.stdin.write(json.dumps(request) + "\n")
        self._process.stdin.flush()
        line = self._process.stdout.readline()
        self.latencies.append(time.perf_counter() - started)
        if not line:
            raise PolicyRefused("the policy process closed its output")

        reply = json.loads(line)
        if "error" in reply:
            raise PolicyRefused(reply["error"])

        chosen = reply["id"]
        # Checked on this side too. The Rust side validates against the offered options, but a
        # trust boundary enforced on one side only is not one.
        if chosen not in choice.ids:
            raise PolicyRefused(
                f"the policy answered {chosen!r}, which was not offered: {choice.ids}"
            )
        return Reply(
            id=chosen,
            label=reply.get("label", ""),
            assigned=int(reply.get("assigned", 0)),
            oov=int(reply.get("oov", 0)),
        )

    def seat(self, fallback: Any) -> "SeatedRustPolicy":
        """Wrap a bot so this policy answers first and `fallback` catches refusals."""
        return SeatedRustPolicy(self, fallback)

    def note(self, message: str) -> None:
        """Say something once, however many decisions repeat it."""
        if message not in self._said:
            self._said.add(message)
            print(f"  [rust policy] {message}", file=sys.stderr)

    def timing(self) -> str:
        """How long decisions took, which is the difference between playable and painful."""
        if not self.latencies:
            return "no decisions timed"
        ordered = sorted(self.latencies)
        median = ordered[len(ordered) // 2]
        return (
            f"{len(ordered)} decision(s): median {median * 1000:.0f} ms, "
            f"slowest {ordered[-1] * 1000:.0f} ms, total {sum(ordered):.1f} s"
        )

    def close(self) -> None:
        if self._process.poll() is None:
            try:
                assert self._process.stdin
                self._process.stdin.close()
                self._process.wait(timeout=5)
            except Exception:
                self._process.kill()

    def __enter__(self) -> "RustPolicy":
        return self

    def __exit__(self, *exc: object) -> None:
        self.close()


class SeatedRustPolicy:
    """One seat's decisions, answered by the Rust policy with a Python bot behind it.

    Only `choose` is overridden. Everything else the driver asks of a bot -- profile installation,
    hand reporting, whatever else it reaches for -- passes through to the wrapped object, so this
    stays a shim rather than a second bot implementation that has to keep up.
    """

    def __init__(self, policy: RustPolicy, fallback: Any) -> None:
        self._policy = policy
        self._fallback = fallback
        self.answered = 0
        self.fell_back = 0

    def choose(self, choice: Any) -> Any:
        try:
            reply = self._policy.ask(choice)
        except PolicyRefused as refusal:
            if self._fallback is None:
                raise
            self.fell_back += 1
            self._policy.note(f"falling back: {refusal}")
            return self._fallback.choose(choice)

        self.answered += 1
        if reply.known < 0.25:
            self._policy.note(
                f"only {reply.known:.0%} of features were known on {choice.prompt!r};"
                " the policy is scoring mostly from out-of-vocabulary columns"
            )
        picked = choice.option(reply.id)
        if picked is None:  # pragma: no cover - ask() already checked
            raise PolicyRefused(f"{reply.id!r} vanished from the choice")
        return picked

    def __getattr__(self, name: str) -> Any:
        return getattr(self._fallback, name)
