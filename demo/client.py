#!/usr/bin/env python3
"""Minimal OpenAI wire-format streaming client; uses only Python's standard library."""
import json
import os
import sys
import urllib.error
import urllib.request


def request(base, token, path, body=None):
    headers = {"Content-Type": "application/json"}
    if token:
        headers["Authorization"] = "Bearer " + token
    data = json.dumps(body).encode() if body is not None else None
    return urllib.request.urlopen(urllib.request.Request(base + path, data=data, headers=headers), timeout=150)


def rejected(base, token):
    try:
        with request(base, token, "/models"):
            raise RuntimeError("expected 401, received success")
    except urllib.error.HTTPError as exc:
        if exc.code != 401:
            raise RuntimeError(f"expected 401, received {exc.code}") from exc
        return exc.code


def stream(base, token, model):
    body = {"model": model, "messages": [{"role": "user", "content": "Explain in one short sentence why an API gateway can be useful."}], "stream": True}
    saw_text = False
    done = False
    with request(base, token, "/chat/completions", body) as response:
        for raw in response:
            line = raw.decode("utf-8").strip()
            if not line.startswith("data:"):
                continue
            payload = line[5:].strip()
            if payload == "[DONE]":
                done = True
                break
            event = json.loads(payload)
            if "error" in event:
                raise RuntimeError("stream returned an error: " + event["error"].get("message", "unknown"))
            for choice in event.get("choices", []):
                content = choice.get("delta", {}).get("content", "")
                if content:
                    saw_text = True
                    print(content, end="", flush=True)
    print(flush=True)
    if not done or not saw_text:
        raise RuntimeError("stream ended without assistant text and [DONE]")


if __name__ == "__main__":
    try:
        stream(os.environ["OPENAI_BASE_URL"].rstrip("/"), os.environ["OPENAI_API_KEY"], os.environ.get("H2A_MODEL", "codex/default"))
    except (OSError, ValueError, RuntimeError, KeyError) as exc:
        print(f"Client failed: {exc}", file=sys.stderr)
        sys.exit(1)
