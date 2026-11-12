"""An ephemeral Modal app and sandbox calling the host's h2a endpoint."""
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from _client import config, run
import modal


def main():
    base, token, model = config(remote=True)
    source = Path(__file__).resolve().parents[1].joinpath("_client.py").read_text()
    secret = modal.Secret.from_dict({"OPENAI_BASE_URL": base, "OPENAI_API_KEY": token, "H2A_MODEL": model})
    with modal.App("h2a-example").run() as app:
        sandbox = modal.Sandbox.create("python", "-c", source, app=app,
                                       image=modal.Image.debian_slim(python_version="3.13"),
                                       secrets=[secret], timeout=300)
        try:
            print("Modal sandbox created; host harness login stays on the host.", flush=True)
            sandbox.wait()
            if sandbox.returncode != 0:
                raise RuntimeError("Sandbox workload failed")
            print(sandbox.stdout.read())
        finally:
            sandbox.terminate()
            print("Modal sandbox stopped.")


if __name__ == "__main__":
    run(main)
