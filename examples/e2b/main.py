"""Send only a scoped gateway key to an ephemeral E2B sandbox."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from _client import config, require_text, run
from e2b import Sandbox


def main():
    base, token, model = config(remote=True)
    sandbox = Sandbox.create(timeout=300, envs={"OPENAI_BASE_URL": base, "OPENAI_API_KEY": token, "H2A_MODEL": model})
    try:
        print("E2B sandbox created; host harness login stays on the host.", flush=True)
        sandbox.files.write("/tmp/h2a-client.py", Path(__file__).resolve().parents[1].joinpath("_client.py").read_text())
        result = sandbox.commands.run("python3 /tmp/h2a-client.py", timeout=170)
        if result.exit_code != 0:
            raise RuntimeError("Sandbox workload failed")
        print(require_text(result.stdout))
    finally:
        sandbox.kill()
        print("E2B sandbox removed.")


if __name__ == "__main__":
    run(main)
