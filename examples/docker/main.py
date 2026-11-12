"""A local container calling a remote HTTPS gateway; no host credentials mounted."""
import os
import subprocess
import sys
import tempfile
import uuid
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from _client import config, run


def main():
    base, token, model = config(remote=True)
    name = "h2a-example-" + uuid.uuid4().hex[:12]
    env = dict(os.environ, OPENAI_BASE_URL=base, OPENAI_API_KEY=token, H2A_MODEL=model)
    source = Path(__file__).resolve().parents[1] / "_client.py"
    ca = os.environ.get("H2A_CA_CERT")
    trust = (["--mount", "type=bind,src=" + str(Path(ca).resolve()) + ",dst=/h2a-ca.pem,readonly",
              "-e", "SSL_CERT_FILE=/h2a-ca.pem"] if ca else [])
    with tempfile.TemporaryDirectory(prefix="h2a-docker-") as tmp:
        cidfile = Path(tmp) / "container-id"
        try:
            result = subprocess.run(["docker", "run", "--rm", "--name", name, "--cidfile", str(cidfile),
                                     "--cap-drop=ALL", "--security-opt=no-new-privileges", "--read-only",
                                     "-e", "OPENAI_BASE_URL", "-e", "OPENAI_API_KEY", "-e", "H2A_MODEL",
                                     "--mount", "type=bind,src=" + str(source) + ",dst=/client.py,readonly",
                                     *trust, "python:3.13-slim", "python", "-B", "/client.py"], env=env, timeout=240)
            if result.returncode:
                raise RuntimeError("Container client failed")
        finally:
            # Without a recorded ID, creation is ambiguous; the name may belong to someone else.
            try:
                container_id = cidfile.read_text().strip()
            except FileNotFoundError:
                container_id = ""
            if len(container_id) == 64 and all(c in "0123456789abcdef" for c in container_id):
                subprocess.run(["docker", "rm", "-f", container_id], stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL, timeout=20)
            else:
                print("Docker did not record a valid container ID; creation status is unknown", file=sys.stderr)


if __name__ == "__main__":
    run(main)
