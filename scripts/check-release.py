#!/usr/bin/env python3
"""Check final archive bytes, then execute the extracted native binary offline."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import signal
import socket
import subprocess
import tarfile
import tempfile
import time
import urllib.error
import urllib.request

from check_readme import check_commands

TARGETS = {
    "x86_64-unknown-linux-gnu": ("linux_amd64", "Linux", "amd64"),
    "aarch64-unknown-linux-gnu": ("linux_arm64", "Linux", "arm64"),
    "x86_64-apple-darwin": ("darwin_amd64", "Darwin", "amd64"),
    "aarch64-apple-darwin": ("darwin_arm64", "Darwin", "arm64"),
    "x86_64-pc-windows-msvc": ("windows_amd64", "Windows", "amd64"),
    "aarch64-pc-windows-msvc": ("windows_arm64", "Windows", "arm64"),
}


def verify(archive, checksum):
    fields = checksum.read_text().strip().split()
    if len(fields) != 2 or fields[1].lstrip("*") != archive.name:
        raise ValueError("Checksum names the wrong archive")
    digest = hashlib.sha256(archive.read_bytes()).hexdigest()
    if digest != fields[0]:
        raise ValueError("Archive checksum mismatch")
    return digest


def main():
    parser = argparse.ArgumentParser()
    parser.add_argument("--version", required=True)
    parser.add_argument("--target", choices=TARGETS, required=True)
    parser.add_argument("--fixture", type=Path, required=True)
    parser.add_argument("--dist", type=Path, default=Path("dist"))
    args = parser.parse_args()
    name, system, arch = TARGETS[args.target]
    machine = platform.machine().lower()
    actual_arch = {"x86_64": "amd64", "amd64": "amd64", "aarch64": "arm64", "arm64": "arm64"}.get(machine)
    if (platform.system(), actual_arch) != (system, arch):
        raise RuntimeError("Wrong native runner for release target")
    archive = args.dist / f"h2a_{args.version}_{name}.tar.gz"
    checksum = args.dist / f"checksums_{args.version}_{name}.txt"
    digest = verify(archive, checksum)
    fixture = args.fixture.resolve(strict=True)
    with tempfile.TemporaryDirectory(prefix="h2a-release-check-") as directory:
        root = Path(directory)
        extracted = root / "archive"
        extracted.mkdir()
        with tarfile.open(archive) as package:
            for member in package.getmembers():
                path = Path(member.name)
                if path.is_absolute() or ".." in path.parts or not (member.isfile() or member.isdir()):
                    raise ValueError("Unsafe archive entry")
            package.extractall(extracted, filter="data")
        binary = extracted / ("h2a.exe" if os.name == "nt" else "h2a")
        if not all((extracted / p).is_file() for p in ("LICENSE", "README.md", "BUILD.txt")):
            raise RuntimeError("Archive missing required files")

        def cli(*parts):
            result = subprocess.run([str(binary), *parts], capture_output=True, text=True, timeout=15)
            if result.returncode:
                raise RuntimeError("Extracted CLI failed: " + parts[0])
            return result.stdout.strip()

        if cli("version") != args.version or "Usage:" not in cli("help"):
            raise RuntimeError("Release version/help mismatch")
        check_commands((extracted / "README.md").read_text(), cli("help"))
        state = root / "state"
        token = cli("keys", "create", "--state-dir", str(state), "--models", "codex/default", "--ttl", "5m", "--max-requests", "1")
        identifier = token.split("_")[1]
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        base = f"http://127.0.0.1:{port}"
        with (root / "server.log").open("w+") as log:
            server = subprocess.Popen([str(binary), "serve", "--state-dir", str(state), "--provider", "codex", "--codex-bin", str(fixture), "--port", str(port)], stdout=log, stderr=log)
            try:
                for _ in range(200):
                    if server.poll() is not None:
                        raise RuntimeError("Extracted gateway exited early")
                    try:
                        with urllib.request.urlopen(base + "/healthz", timeout=.2):
                            break
                    except OSError:
                        time.sleep(.05)
                else:
                    raise RuntimeError("Extracted gateway readiness timed out")

                def request(key):
                    headers = {"Content-Type": "application/json"}
                    if key:
                        headers["Authorization"] = "Bearer " + key
                    req = urllib.request.Request(base + "/v1/chat/completions", data=json.dumps({"model": "codex/default", "messages": [{"role": "user", "content": "Offline release verification"}]}).encode(), headers=headers)
                    try:
                        with urllib.request.urlopen(req, timeout=20) as response:
                            return response.status, json.load(response)
                    except urllib.error.HTTPError as error:
                        return error.code, json.load(error)

                if request(None)[0] != 401:
                    raise RuntimeError("Missing key was not rejected")
                status, response = request(token)
                if status != 200 or response["choices"][0]["message"]["content"] != "Hello 🌍":
                    raise RuntimeError("Extracted gateway fixture completion failed")
                if request(token)[0] != 429:
                    raise RuntimeError("Request quota was not enforced")
                if json.loads(cli("keys", "inspect", "--state-dir", str(state), identifier))["accepted"] != 1:
                    raise RuntimeError("Durable accounting mismatch")
                cli("keys", "revoke", "--state-dir", str(state), identifier)
                if request(token)[0] != 401:
                    raise RuntimeError("Revoked key was not rejected")
            finally:
                if os.name == "nt":
                    server.terminate()
                else:
                    server.send_signal(signal.SIGINT)
                try:
                    server.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()
            log.flush()
            log.seek(0)
            if token in log.read():
                raise RuntimeError("Credential found in gateway logs")
    evidence = {"version": args.version, "target": args.target, "system": platform.system(), "machine": machine, "archive": archive.name, "sha256": digest, "checks": ["checksum", "extract", "version", "help", "readme-command-reference", "missing-key-401", "fixture-completion", "quota-429", "durable-count", "revoked-key-401", "secret-free-logs"], "live_provider": False}
    (args.dist / f"verification_{args.version}_{name}.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print("PASS " + name + ": extracted archive version, HTTP and bounded-key lifecycle")


if __name__ == "__main__":
    main()
