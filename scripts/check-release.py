#!/usr/bin/env python3
"""Check final archive bytes, then execute the extracted native binary offline."""
import argparse
import hashlib
import importlib.util
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
        if not all((extracted / p).is_file() for p in ("LICENSE", "README.md", "BUILD.txt", "scripts/check-tool-calling.py")):
            raise RuntimeError("Archive missing required files")

        def cli(*parts):
            result = subprocess.run([str(binary), *parts], capture_output=True, text=True, timeout=15)
            if result.returncode:
                raise RuntimeError("Extracted CLI failed: " + parts[0])
            return result.stdout.strip()

        if cli("version") != args.version or "Usage:" not in cli("help"):
            raise RuntimeError("Release version/help mismatch")
        check_commands((extracted / "README.md").read_text(), cli("help"))
        spec = importlib.util.spec_from_file_location("packaged_example_docs", extracted / "scripts/check-example-docs.py")
        example_docs = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(example_docs)
        if example_docs.check(extracted):
            raise RuntimeError("Packaged example commands, dependencies or links are invalid")
        state = root / "state"
        text_token = cli("keys", "create", "--state-dir", str(state), "--models", "codex/default", "--ttl", "5m", "--max-requests", "1")
        text_identifier = text_token.split("_")[1]
        token = cli("keys", "create", "--state-dir", str(state), "--models", "codex/default", "--ttl", "5m", "--max-requests", "2")
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

                body = {
                    "model": "codex/default",
                    "messages": [{"role": "user", "content": "Call lookup_status for service api, then report the result."}],
                    "tools": [{"type": "function", "function": {
                        "name": "lookup_status", "strict": True,
                        "parameters": {"type": "object", "properties": {"service": {"type": "string"}},
                                       "required": ["service"], "additionalProperties": False},
                    }}],
                    "tool_choice": "required", "parallel_tool_calls": False,
                }

                def request(key, payload=body):
                    headers = {"Content-Type": "application/json"}
                    if key:
                        headers["Authorization"] = "Bearer " + key
                    req = urllib.request.Request(base + "/v1/chat/completions", data=json.dumps(payload).encode(), headers=headers)
                    try:
                        with urllib.request.urlopen(req, timeout=20) as response:
                            return response.status, json.load(response)
                    except urllib.error.HTTPError as error:
                        return error.code, json.load(error)

                if request(None)[0] != 401:
                    raise RuntimeError("Missing key was not rejected")
                text_body = {"model": "codex/default", "messages": [{"role": "user", "content": "Offline release verification"}]}
                status, response = request(text_token, text_body)
                if status != 200 or response["choices"][0]["message"]["content"] != "Hello 🌍":
                    raise RuntimeError("Extracted gateway fixture completion failed")
                if json.loads(cli("keys", "inspect", "--state-dir", str(state), text_identifier))["accepted"] != 1:
                    raise RuntimeError("Text completion accounting mismatch")
                if request(text_token, text_body)[0] != 429:
                    raise RuntimeError("Text request quota was not enforced")
                cli("keys", "revoke", "--state-dir", str(state), text_identifier)
                if request(text_token, text_body)[0] != 401:
                    raise RuntimeError("Revoked text key was not rejected")
                status, response = request(token)
                if status != 200:
                    raise RuntimeError("Extracted gateway tool request failed")
                choices = response.get("choices")
                if not isinstance(choices, list) or len(choices) != 1 or not isinstance(choices[0], dict):
                    raise RuntimeError("Invalid tool response choices")
                choice = choices[0]
                message = choice.get("message")
                if choice.get("finish_reason") != "tool_calls" or not isinstance(message, dict) or message.get("role") != "assistant" or message.get("content", "missing") is not None:
                    raise RuntimeError("Invalid assistant tool response")
                calls = message.get("tool_calls")
                if not isinstance(calls, list) or len(calls) != 1 or not isinstance(calls[0], dict):
                    raise RuntimeError("Invalid tool calls")
                call = calls[0]
                function = call.get("function")
                if (not isinstance(call.get("id"), str) or not call["id"] or call.get("type") != "function"
                        or not isinstance(function, dict) or function.get("name") != "lookup_status"
                        or not isinstance(function.get("arguments"), str)):
                    raise RuntimeError("Invalid function call")
                try:
                    arguments = json.loads(function["arguments"])
                except ValueError as error:
                    raise RuntimeError("Invalid function arguments") from error
                if arguments != {"service": "api"}:
                    raise RuntimeError("Unexpected function arguments")
                result = {"api": "operational"}[arguments["service"]]
                body["messages"].extend([message, {"role": "tool", "tool_call_id": call["id"], "content": result}])
                body["tool_choice"] = "auto"
                status, response = request(token)
                choices = response.get("choices")
                if (status != 200 or not isinstance(choices, list) or len(choices) != 1
                        or not isinstance(choices[0], dict) or choices[0].get("finish_reason") != "stop"
                        or not isinstance(choices[0].get("message"), dict)
                        or choices[0]["message"].get("role") != "assistant"
                        or choices[0]["message"].get("content") != "Service api is operational."):
                    raise RuntimeError("Extracted gateway tool replay failed")
                if json.loads(cli("keys", "inspect", "--state-dir", str(state), identifier))["accepted"] != 2:
                    raise RuntimeError("Durable accounting mismatch")
                if request(token)[0] != 429:
                    raise RuntimeError("Request quota was not enforced")
                if json.loads(cli("keys", "inspect", "--state-dir", str(state), identifier))["accepted"] != 2:
                    raise RuntimeError("Quota refusal changed accounting")
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
            logs = log.read()
            if token in logs or text_token in logs:
                raise RuntimeError("Credential found in gateway logs")
    evidence = {"version": args.version, "target": args.target, "system": platform.system(), "machine": machine, "archive": archive.name, "sha256": digest, "checks": ["checksum", "extract", "version", "help", "readme-command-reference", "example-documentation", "missing-key-401", "fixture-completion", "fixture-tool-call", "fixture-tool-replay", "quota-429", "durable-count", "revoked-key-401", "secret-free-logs"], "live_provider": False}
    (args.dist / f"verification_{args.version}_{name}.json").write_text(json.dumps(evidence, indent=2) + "\n")
    print("PASS " + name + ": extracted archive version, text, tool loop and bounded-key lifecycle")


if __name__ == "__main__":
    main()
