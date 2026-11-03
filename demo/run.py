#!/usr/bin/env python3
import argparse
import json
import os
from pathlib import Path
import socket
import signal
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

from client import rejected, stream

ROOT = Path(__file__).resolve().parent.parent


def main():
    parser = argparse.ArgumentParser(description="Run an isolated h2a authentication and streaming demo")
    parser.add_argument("--fixture", action="store_true", help="use an offline fake Codex CLI; no provider call")
    parser.add_argument("--provider", choices=["codex", "antigravity"], default="codex")
    parser.add_argument("--model", help="full provider/model ID (default: provider/default)")
    args = parser.parse_args()
    model = args.model or args.provider + "/default"
    if not model.startswith(args.provider + "/"):
        parser.error("model must match --provider")
    if args.fixture and args.provider != "codex":
        parser.error("the fixture only implements Codex")
    with tempfile.TemporaryDirectory(prefix="h2a-demo-") as temp:
        state = Path(temp) / "state"
        binary = os.environ.get("H2A_BIN")
        if binary:
            binary = str(Path(binary).expanduser().resolve())
        else:
            binary = str(Path(temp) / "h2a")
            print("Building h2a…", flush=True)
            subprocess.run(["go", "build", "-o", binary, "./cmd/h2a"], cwd=ROOT, check=True)
        label = "OFFLINE FIXTURE — no provider call" if args.fixture else "LIVE " + args.provider.upper() + " — uses your existing harness login"
        print(f"\n{label}\nModel: {model}", flush=True)
        result = subprocess.run([binary, "keys", "create", "--state-dir", str(state), "--name", "demo", "--models", model], stdout=subprocess.PIPE, stderr=subprocess.PIPE, check=True, text=True)
        token = result.stdout.strip()
        records = json.loads(subprocess.check_output([binary, "keys", "list", "--state-dir", str(state)], text=True))
        key_id = records[0]["id"]
        print("Created an isolated, model-scoped key (secret hidden).", flush=True)
        # Bind port 0 to find a free local port, then release it for h2a. A concurrent
        # process could take it; server startup failure is detected below.
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        base = f"http://127.0.0.1:{port}"
        command = [binary, "serve", "--state-dir", str(state), "--provider", args.provider, "--port", str(port)]
        if args.fixture:
            command += ["--codex-bin", str(ROOT / "demo/fixture-codex.sh")]
        log = Path(temp) / "server.log"
        with log.open("w+") as server_log:
            server = subprocess.Popen(command, stdout=server_log, stderr=server_log)
            try:
                for _ in range(100):
                    if server.poll() is not None:
                        server_log.flush()
                        raise RuntimeError("gateway exited during startup: " + log.read_text())
                    try:
                        with urllib.request.urlopen(base + "/healthz", timeout=0.2) as health:
                            if health.status == 200:
                                break
                    except (OSError, urllib.error.URLError):
                        time.sleep(0.1)
                else:
                    raise RuntimeError("gateway did not become ready")
                print("Gateway ready on loopback.\n", flush=True)
                print(f"No key → HTTP {rejected(base + '/v1', None)}", flush=True)
                print("\nStreaming answer:", flush=True)
                stream(base + "/v1", token, model)
                subprocess.run([binary, "keys", "revoke", "--state-dir", str(state), key_id], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, check=True)
                print(f"\nRevoked key → HTTP {rejected(base + '/v1', token)}", flush=True)
                print("Done. Gateway and temporary key store are being removed.", flush=True)
            finally:
                server.terminate()
                try:
                    server.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()


if __name__ == "__main__":
    # Turn termination into stack unwinding so the gateway and temp directory close.
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
    try:
        main()
    except (OSError, ValueError, RuntimeError, subprocess.CalledProcessError) as exc:
        print(f"Demo failed: {exc}", file=sys.stderr)
        sys.exit(1)
