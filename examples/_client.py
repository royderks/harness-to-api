"""Shared configuration and a dependency-free sandbox workload."""
import json
import os
import sys
import urllib.error
import urllib.parse
import urllib.request

PROMPT = "Write a three-bullet incident update for a checkout outage: cause unknown, rollback underway, next update in 15 minutes. Do not invent facts."


def config(remote=False):
    base = os.environ.get("OPENAI_BASE_URL", "http://127.0.0.1:8787/v1").rstrip("/")
    url = urllib.parse.urlsplit(base)
    local = url.scheme == "http" and url.hostname in ("127.0.0.1", "::1")
    if (url.scheme != "https" and (remote or not local)) or not url.hostname:
        raise ValueError("Use HTTPS for remote clients, or literal loopback HTTP locally")
    if url.username or url.password or url.query or url.fragment or url.path != "/v1":
        raise ValueError("OPENAI_BASE_URL must end in /v1 without credentials, query or fragment")
    token = os.environ.get("OPENAI_API_KEY", "")
    if not token:
        raise ValueError("Set OPENAI_API_KEY to a scoped h2a gateway key")
    return base, token, os.environ.get("H2A_MODEL", "codex/default")


def sdk_client():
    import httpx
    from openai import OpenAI
    base, token, _ = config()
    return OpenAI(base_url=base, api_key=token, max_retries=0, timeout=150,
                  http_client=httpx.Client(follow_redirects=False, trust_env=False))


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        return None


def require_text(value):
    if not isinstance(value, str) or not value.strip():
        raise ValueError("Completion contained no text")
    return value


def complete():
    base, token, model = config()
    request = urllib.request.Request(base + "/chat/completions", data=json.dumps({
        "model": model, "messages": [{"role": "user", "content": PROMPT}]
    }).encode(), headers={"Content-Type": "application/json", "Authorization": "Bearer " + token})
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}), NoRedirect())
    with opener.open(request, timeout=150) as response:
        result = json.load(response)
    print(require_text(result["choices"][0]["message"]["content"]))


def run(main):
    try:
        main()
    except KeyboardInterrupt:
        sys.exit(130)
    except Exception as exc:
        # SDK exceptions can include request/response bodies. Keep recording diagnostics private.
        status = getattr(exc, "status_code", getattr(exc, "code", None))
        print("Demo failed (" + type(exc).__name__ + (", HTTP " + str(status) if isinstance(status, int) else "") + "). Check setup, model scope and remaining quota.", file=sys.stderr)
        sys.exit(1)


if __name__ == "__main__":
    run(complete)
