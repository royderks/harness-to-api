"""One client, multiple installed harnesses. One accepted request per model."""
import os
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from _client import PROMPT, run, sdk_client


def main():
    models = os.environ.get("H2A_MODELS", "codex/default,gemini/default").split(",")
    with sdk_client() as client:
        for model in models:
            model = model.strip()
            print("\n--- " + model + " ---", flush=True)
            with client.chat.completions.create(model=model, messages=[{"role": "user", "content": PROMPT}], stream=True) as stream:
                for chunk in stream:
                    if chunk.choices:
                        print(chunk.choices[0].delta.content or "", end="", flush=True)
            print()


if __name__ == "__main__":
    run(main)
