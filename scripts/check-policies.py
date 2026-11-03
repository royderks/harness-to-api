#!/usr/bin/env python3
"""Exercise bounded keys through the built CLI, HTTP, and an offline CLI fixture."""
import concurrent.futures
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import subprocess
import tempfile
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(os.environ.get("H2A_BIN", ROOT / "bin/h2a")).resolve()
FIXTURE = Path(os.environ.get("H2A_FIXTURE", ROOT / "target/debug/examples/harness-fixture")).resolve()


def main():
    with tempfile.TemporaryDirectory(prefix="h2a-policy-proof-") as directory:
        state = Path(directory)
        fixture = state / "fixture"
        shutil.copy2(FIXTURE, fixture)
        secrets = []
        server = None
        log = (state / "server.log").open("w+")
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        base = f"http://127.0.0.1:{port}"

        def cli(*args):
            result = subprocess.run([str(BIN), *args], text=True, capture_output=True, timeout=10)
            if result.returncode:
                raise AssertionError(f"CLI {args[0]} failed (exit {result.returncode})")
            return result.stdout

        def key(*limits):
            token = cli("keys", "create", "--state-dir", str(state), "--models", "codex/*", *limits).strip()
            secrets.append(token)
            return token, token.split("_")[1]

        def request(token, model="codex/fixture", stream=False, extra=None):
            body = {"model": model, "messages": [{"role": "user", "content": "Offline fixture."}], "stream": stream}
            body.update(extra or {})
            req = urllib.request.Request(base + "/v1/chat/completions", data=json.dumps(body).encode(), headers={"Authorization": "Bearer " + token, "Content-Type": "application/json"})
            try:
                with urllib.request.urlopen(req, timeout=8) as response:
                    return response.status, response.read().decode()
            except urllib.error.HTTPError as error:
                return error.code, error.read().decode()

        def inspect(identifier):
            return json.loads(cli("keys", "inspect", "--state-dir", str(state), identifier))

        def start():
            nonlocal server
            server = subprocess.Popen([str(BIN), "serve", "--state-dir", str(state), "--provider", "codex", "--codex-bin", str(fixture), "--port", str(port), "--allowed-models", "codex/fixture,codex/hang"], stdout=log, stderr=log)
            deadline = time.monotonic() + 5
            while time.monotonic() < deadline:
                if server.poll() is not None:
                    raise AssertionError("Gateway startup failed")
                try:
                    with urllib.request.urlopen(base + "/healthz", timeout=.2):
                        return
                except (OSError, urllib.error.URLError):
                    time.sleep(.03)
            raise AssertionError("Gateway startup timed out")

        def stop():
            nonlocal server
            if server is not None:
                server.send_signal(signal.SIGINT)
                try:
                    server.wait(timeout=8)
                finally:
                    if server.poll() is None:
                        server.kill()
                        server.wait()
                server = None

        try:
            quota, quota_id = key("--max-requests", "2")
            start()
            assert request(quota, extra={"temperature": .5})[0] == 400
            assert request(quota, model="codex/unconfigured")[0] == 404
            assert inspect(quota_id)["accepted"] == 0
            assert request(quota)[0] == 200
            status, body = request(quota, stream=True)
            assert status == 200 and "[DONE]" in body
            status, body = request(quota)
            assert status == 429 and "request_quota_exceeded" in body
            explanation = json.loads(cli("keys", "explain", "--state-dir", str(state), "--model", "codex/fixture", "--allowed-models", "codex/fixture", quota_id))
            assert explanation["model_allowed"] and not explanation["local_policy_allowed"]
            assert explanation["account_access"] == "unknown"
            assert inspect(quota_id)["accepted"] == 2
            stop()
            start()
            assert request(quota)[0] == 429
            print("PASS: invalid requests and explanations do not debit; lifetime quota survives restart")

            rate, _ = key("--rate-limit", "1/min")
            assert request(rate)[0] == 200
            status, body = request(rate)
            assert status == 429 and "rate_limit_exceeded" in body
            print("PASS: rolling request-rate limit")

            timed, _ = key("--request-timeout", "150ms")
            status, body = request(timed, model="codex/hang", stream=True)
            assert status == 200 and "harness_timeout" in body and "[DONE]" not in body
            print("PASS: request timeout cancels fixture process and errors the stream")

            expiring, _ = key("--ttl", "300ms")
            status, body = request(expiring, model="codex/hang", stream=True)
            assert status == 200 and "[DONE]" not in body and '"error"' in body
            assert request(expiring)[0] == 401
            print("PASS: expiry cancels an active stream and rejects subsequent requests")

            revocable, identifier = key("--max-concurrent", "1")
            with concurrent.futures.ThreadPoolExecutor(max_workers=1) as executor:
                running = executor.submit(request, revocable, "codex/hang")
                deadline = time.monotonic() + 3
                while inspect(identifier)["accepted"] == 0:
                    assert time.monotonic() < deadline, "Run was never admitted"
                    time.sleep(.02)
                status, body = request(revocable)
                assert status == 429 and "key_concurrency_exceeded" in body
                cli("keys", "revoke", "--state-dir", str(state), identifier)
                status, body = running.result(timeout=4)
                assert status == 401 and "invalid_api_key" in body
            print("PASS: key concurrency and separate-process revocation cancel a running CLI")

            probe, probe_id = key("--max-requests", "1")
            env = dict(os.environ, OPENAI_API_KEY=probe)
            result = subprocess.run([str(BIN), "probe", "--base-url", base + "/v1", "--model", "codex/fixture"], env=env, text=True, capture_output=True, timeout=10)
            assert result.returncode == 0
            assert inspect(probe_id)["accepted"] == 1
            assert all(secret not in result.stdout + result.stderr for secret in secrets)
            print("PASS: explicit probe uses ordinary quota admission and keeps the key private")
            stop()
            log.flush()
            log.seek(0)
            diagnostics = log.read()
            assert all(secret not in diagnostics for secret in secrets)
            print("PASS: gateway diagnostics contain no generated keys (offline fixture; no provider inference)")
        finally:
            stop()
            log.close()


if __name__ == "__main__":
    main()
