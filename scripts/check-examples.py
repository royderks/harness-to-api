#!/usr/bin/env python3
"""Run real examples clients against h2a and six canned harness protocols."""
import json
import os
from pathlib import Path
import signal
import shlex
import socket
import subprocess
import sys
import tempfile
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
EXAMPLES = ROOT / "examples"
BIN = Path(os.environ.get("H2A_BIN", ROOT / "target/debug/h2a")).resolve()
FIXTURE = Path(os.environ.get("H2A_FIXTURE", ROOT / "target/debug/examples/harness-fixture")).resolve()
PROVIDERS = {"codex": "codex", "antigravity": "antigravity", "ibm-bob": "pi", "gemini": "gemini", "claude": "claude", "opencode": "opencode"}


def main():
    subprocess.run([sys.executable, str(ROOT / "scripts/check-example-docs.py")], check=True)
    print("OFFLINE FIXTURES — real h2a and SDKs, canned harness output; no cloud sandbox or provider calls.", flush=True)
    with tempfile.TemporaryDirectory(prefix="h2a-examples-") as directory:
        state = Path(directory)
        extension = state / "bob/extensions"
        extension.mkdir(parents=True)
        (extension / "bob.ts").write_text("// Offline fixture; never executed.\n")

        def cli(*args):
            result = subprocess.run([str(BIN), *args[:2], "--state-dir", str(state / "state"), *args[2:]], capture_output=True, text=True, timeout=10)
            if result.returncode:
                raise RuntimeError("h2a command failed: " + args[0])
            return result.stdout.strip()

        models = ",".join(p + ("/premium" if p == "ibm-bob" else "/default") for p in PROVIDERS)
        token = cli("keys", "create", "--name", "examples-fixture", "--models", models,
                    "--ttl", "10m", "--max-requests", "10", "--request-timeout", "20s", "--max-concurrent", "1")
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        base = f"http://127.0.0.1:{port}"
        command = [str(BIN), "serve", "--state-dir", str(state / "state"), "--provider", "all", "--port", str(port), "--pi-bob-extension", str(extension.parent)]
        for flag in PROVIDERS.values():
            command.extend(["--" + flag + "-bin", str(FIXTURE)])
        with (state / "server.log").open("w+") as log:
            server = subprocess.Popen(command, stdout=log, stderr=log)
            try:
                for _ in range(100):
                    if server.poll() is not None:
                        raise RuntimeError("h2a startup failed")
                    try:
                        with urllib.request.urlopen(base + "/healthz", timeout=.2):
                            break
                    except OSError:
                        time.sleep(.05)
                else:
                    raise RuntimeError("h2a startup timed out")
                env = dict(os.environ, OPENAI_BASE_URL=base + "/v1", OPENAI_API_KEY=token,
                           H2A_MODEL="codex/default", H2A_MODELS=models, H2A_REVIEW_MODEL="gemini/default")
                # Do not let a developer's optional SDK tracing settings export fixture data.
                env.update(LANGSMITH_TRACING="false", LANGCHAIN_TRACING_V2="false", OPENAI_AGENTS_DISABLE_TRACING="1")
                manifest = json.loads((EXAMPLES / "commands.json").read_text())
                commands = [entry["commands"][0] for entry in manifest.values() if entry["offline"]]
                commands.append("python examples/_client.py")
                for documented in commands:
                    argv = shlex.split(documented)
                    if len(argv) != 2 or argv[0] != "python":
                        raise RuntimeError("Invalid offline command: " + documented)
                    script = argv[1]
                    result = subprocess.run([sys.executable, script], cwd=ROOT, env=env, capture_output=True, text=True, timeout=90)
                    if token in result.stdout + result.stderr:
                        raise RuntimeError("Credential appeared in client output")
                    if result.returncode or "Hello 🌍" not in result.stdout:
                        print(result.stderr, file=sys.stderr)
                        raise RuntimeError("Client failed: " + script)
                    print("PASS " + script, flush=True)
                snapshot = json.loads(cli("keys", "inspect", token.split("_")[1]))
                # Six switched models + two graph nodes + one agent + one plain HTTP client.
                if snapshot["accepted"] != 10:
                    raise RuntimeError("Unexpected request count; client retries or extra turns")
                print("PASS exactly 10 accepted requests; no hidden retries", flush=True)
                for label, mutate in (("quota", None), ("revocation", lambda: cli("keys", "revoke", token.split("_")[1]))):
                    if mutate:
                        mutate()
                    result = subprocess.run([sys.executable, str(EXAMPLES / "_client.py")], env=env, capture_output=True, text=True, timeout=10)
                    expected = "429" if label == "quota" else "401"
                    if result.returncode != 1 or expected not in result.stderr or token in result.stdout + result.stderr:
                        raise RuntimeError("Denial check failed: " + label)
                    print("PASS " + label + " HTTP " + expected, flush=True)
            finally:
                server.send_signal(signal.SIGINT)
                try:
                    server.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()
    print("PASS gateway and temporary keys cleaned up")


if __name__ == "__main__":
    main()
