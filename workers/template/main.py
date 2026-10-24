"""Template worker: echoes the request back. Copy this folder to create a new worker.

See workers/README.md for the protocol.
"""

import json
import sys


def emit(**message) -> None:
    print(json.dumps(message), flush=True)


def handle(request: dict) -> dict:
    # Do the actual work here, reporting progress along the way.
    emit(progress=1.0)
    return {"echo": request}


if __name__ == "__main__":
    try:
        emit(result=handle(json.load(sys.stdin)))
    except Exception as e:  # report any failure through the protocol
        emit(error=f"{type(e).__name__}: {e}")
        sys.exit(1)
