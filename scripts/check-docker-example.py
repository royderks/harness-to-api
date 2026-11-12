#!/usr/bin/env python3
"""Exercise the real Docker example through TLS and an offline h2a fixture."""
import http.client
import http.server
import json
import os
from pathlib import Path
import shutil
import signal
import socket
import ssl
import subprocess
import sys
import tempfile
import threading
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(os.environ["H2A_BIN"]).resolve()
FIXTURE = Path(os.environ["H2A_FIXTURE"]).resolve()


def command(args, **kwargs):
    return subprocess.run(args, check=True, timeout=30, **kwargs)


def main():
    docker = shutil.which("docker")
    if not docker:
        raise RuntimeError("Docker is required")
    command([docker, "info"], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
    if sys.platform == "darwin":
        hostname = "host.docker.internal"
        san = "DNS:" + hostname
    elif sys.platform.startswith("linux"):
        hostname = command([docker, "network", "inspect", "bridge", "--format", "{{(index .IPAM.Config 0).Gateway}}"], capture_output=True, text=True).stdout.strip()
        if not hostname:
            raise RuntimeError("Docker bridge gateway unavailable")
        san = "IP:" + hostname
    else:
        raise RuntimeError("Docker smoke supports Linux and macOS hosts")

    with tempfile.TemporaryDirectory(prefix=".h2a-docker-", dir=ROOT) as tmp:
        root = Path(tmp)
        state = root / "state"
        cert, key = root / "cert.pem", root / "key.pem"
        command(["openssl", "req", "-x509", "-newkey", "rsa:2048", "-nodes", "-keyout", str(key), "-out", str(cert), "-days", "1", "-subj", "/CN=h2a-fixture", "-addext", "subjectAltName=" + san], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)

        def cli(*args):
            return command([str(BIN), *args[:2], "--state-dir", str(state), *args[2:]], capture_output=True, text=True).stdout.strip()

        token = cli("keys", "create", "--name", "docker-fixture", "--models", "codex/default", "--ttl", "10m", "--max-requests", "1", "--request-timeout", "20s")
        with socket.socket() as sock:
            sock.bind(("127.0.0.1", 0))
            port = sock.getsockname()[1]
        server = subprocess.Popen([str(BIN), "serve", "--state-dir", str(state), "--provider", "codex", "--port", str(port), "--codex-bin", str(FIXTURE)], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        proxy = None
        names = root / "names"
        try:
            for _ in range(100):
                if server.poll() is not None:
                    raise RuntimeError("h2a startup failed")
                try:
                    with urllib.request.urlopen(f"http://127.0.0.1:{port}/healthz", timeout=.2):
                        break
                except OSError:
                    time.sleep(.05)
            else:
                raise RuntimeError("h2a startup timed out")

            class Proxy(http.server.BaseHTTPRequestHandler):
                def do_POST(self):
                    connection = http.client.HTTPConnection("127.0.0.1", port, timeout=30)
                    try:
                        body = self.rfile.read(int(self.headers["Content-Length"]))
                        connection.request("POST", self.path, body, dict(self.headers))
                        response = connection.getresponse()
                        data = response.read()
                        self.send_response(response.status)
                        self.send_header("Content-Length", str(len(data)))
                        self.end_headers()
                        self.wfile.write(data)
                    finally:
                        connection.close()

                def log_message(self, *args):
                    pass

            proxy = http.server.ThreadingHTTPServer(("0.0.0.0", 0), Proxy)
            proxy.daemon_threads = True
            context = ssl.SSLContext(ssl.PROTOCOL_TLS_SERVER)
            context.load_cert_chain(cert, key)
            proxy.socket = context.wrap_socket(proxy.socket, server_side=True)
            thread = threading.Thread(target=proxy.serve_forever, daemon=True)
            thread.start()

            # Record only generated container names; the real Docker executable receives the original argv.
            wrapper = root / "docker"
            wrapper.write_text("#!/usr/bin/env python3\nimport os, sys\na = sys.argv[1:]\nif a and a[0] == 'run':\n with open(os.environ['H2A_TEST_NAMES'], 'a') as f: f.write(a[a.index('--name') + 1] + '\\n')\nos.execv(os.environ['H2A_TEST_DOCKER'], [os.environ['H2A_TEST_DOCKER'], *a])\n")
            wrapper.chmod(0o755)
            env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ["PATH"], H2A_TEST_DOCKER=docker, H2A_TEST_NAMES=str(names), OPENAI_BASE_URL=f"https://{hostname}:{proxy.server_port}/v1", OPENAI_API_KEY=token, H2A_MODEL="codex/default", H2A_CA_CERT=str(cert))

            def run_example(label, expected, status=None, ca=True):
                run_env = env.copy()
                if not ca:
                    run_env.pop("H2A_CA_CERT")
                result = subprocess.run([sys.executable, str(ROOT / "examples/docker/main.py")], env=run_env, capture_output=True, text=True, timeout=260)
                output = result.stdout + result.stderr
                if token in output:
                    raise RuntimeError("Credential appeared in client output")
                if result.returncode != expected or (status and status not in output):
                    raise RuntimeError(label + " failed (exit " + str(result.returncode) + "; expected output missing or exit differed)")
                name = names.read_text().splitlines()[-1]
                listing = command([docker, "ps", "-a", "--filter", "name=^/" + name + "$", "--format", "{{.Names}}"], capture_output=True, text=True).stdout.splitlines()
                if name in listing:
                    raise RuntimeError(label + " left a container behind")
                print("PASS " + label + "; container removed", flush=True)

            run_example("untrusted TLS rejected", 1, "URLError", ca=False)
            run_example("fixture completion", 0, "Hello 🌍")
            snapshot = json.loads(cli("keys", "inspect", token.split("_")[1]))
            if snapshot["accepted"] != 1:
                raise RuntimeError("Unexpected accepted request count")
            run_example("quota HTTP 429", 1, "HTTP 429")
            cli("keys", "revoke", token.split("_")[1])
            run_example("revocation HTTP 401", 1, "HTTP 401")
        finally:
            if names.exists():
                for name in names.read_text().splitlines():
                    if name.startswith("h2a-example-") and len(name) == len("h2a-example-") + 12 and all(c in "0123456789abcdef" for c in name[12:]):
                        try:
                            subprocess.run([docker, "rm", "-f", name], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=20)
                        except (OSError, subprocess.TimeoutExpired):
                            print("ISSUE: could not clean up owned container " + name, file=sys.stderr)
            if proxy:
                proxy.shutdown()
                proxy.server_close()
            server.send_signal(signal.SIGINT)
            try:
                server.wait(timeout=10)
            except subprocess.TimeoutExpired:
                server.kill()
                server.wait()
    print("PASS gateway and temporary state cleaned up; fixture only")


if __name__ == "__main__":
    main()
