"""Run the bridge endpoint.

    python bridge/run_bridge.py [--port 8080] [--capture out/bridge-captures]

The endpoint is `server.py`, vendored from the historical repository and not ported. This is the
entry point it never had: the old repository always started it from inside a test or a longer
session, and running the bridge on its own is now the normal case, because everything above it is
Rust.

It binds 127.0.0.1 and has no authentication of any kind. Anything that can reach the port can move
pieces on the table, which is why it is loopback and why `--host` is deliberately not an option.

Ctrl-C stops it. `--capture` writes every upload to a directory, which is how a board is kept for
replay without a live table.
"""

from __future__ import annotations

import argparse
import sys
import time
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent.parent))

from bridge.server import DEFAULT_PORT, FEATURES, BridgeServer  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    parser.add_argument(
        "--capture",
        type=Path,
        default=None,
        help="write every upload here, for replay without a table",
    )
    arguments = parser.parse_args()

    with BridgeServer(port=arguments.port, capture_dir=arguments.capture) as server:
        print(f"bridge listening on 127.0.0.1:{server.port}")
        print(f"  features   {', '.join(FEATURES)}")
        print(f"  the mod posts to /postkey, /posttimestamp and /data")
        print(f"  the executor polls /poll every two seconds")
        print(f"  Rust reads /latest and queues to /queue")
        if arguments.capture:
            print(f"  capturing  {arguments.capture}")
        print("Ctrl-C to stop.")
        try:
            while True:
                time.sleep(3600)
        except KeyboardInterrupt:
            print("\nstopping")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
