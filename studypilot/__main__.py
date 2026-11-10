"""StudyPilot server and desktop entrypoint."""

from __future__ import annotations

import argparse
import socket
import threading
import time
import webbrowser

import uvicorn

from .app import create_app
from .config import DEFAULT_PORT, HOST, PORT_RANGE


def pick_port(preferred: int = DEFAULT_PORT) -> int:
    candidates = [preferred, *[port for port in PORT_RANGE if port != preferred]]
    for port in candidates:
        with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
            try:
                sock.bind((HOST, port))
                return port
            except OSError:
                continue
    with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as sock:
        sock.bind((HOST, 0))
        return int(sock.getsockname()[1])


def run_server(host: str, port: int, *, desktop: bool = False) -> None:
    app = create_app()
    url = f"http://{host}:{port}/"
    if desktop:
        try:
            import webview

            threading.Thread(
                target=lambda: uvicorn.run(
                    app,
                    host=host,
                    port=port,
                    log_level="warning",
                ),
                daemon=True,
            ).start()
            time.sleep(1.0)
            webview.create_window("StudyPilot 学习任务中心", url, width=1280, height=860)
            webview.start()
            return
        except ImportError:
            pass
    uvicorn.run(app, host=host, port=port, log_level="info")


def main(argv: list[str] | None = None) -> int:
    parser = argparse.ArgumentParser(description="StudyPilot 学习任务中心")
    parser.add_argument("--host", default=HOST)
    parser.add_argument("--port", type=int, default=DEFAULT_PORT)
    parser.add_argument("--no-browser", action="store_true")
    parser.add_argument("--desktop", action="store_true")
    args = parser.parse_args(argv)
    port = pick_port(args.port)
    if not args.no_browser and not args.desktop:
        threading.Timer(
            1.2,
            lambda: webbrowser.open(f"http://{args.host}:{port}/"),
        ).start()
    run_server(args.host, port, desktop=args.desktop)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())

