"""One client, multiple installed harnesses. One accepted request per model."""
import os
import sys
from pathlib import Path
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from _client import PROMPT, require_text, run, sdk_client


def main():
    models = os.environ.get("H2A_MODELS", "codex/default,gemini/default").split(",")
    with sdk_client() as client:
        for model in models:
            model = model.strip()
            print("\n--- " + model + " ---", flush=True)
            parts = []
            finish_reason = None
            with client.chat.completions.create(model=model, messages=[{"role": "user", "content": PROMPT}], stream=True) as stream:
                for chunk in stream:
                    if chunk.choices:
                        choice = chunk.choices[0]
                        if choice.finish_reason is not None:
                            finish_reason = choice.finish_reason
                        text = choice.delta.content or ""
                        parts.append(text)
                        print(text, end="", flush=True)
            require_text("".join(parts))
            if finish_reason != "stop":
                raise RuntimeError("Stream did not finish successfully")
            print()


if __name__ == "__main__":
    run(main)
