# Install the client dependency: pip install openai
import os
from openai import OpenAI

client = OpenAI(
    base_url=os.getenv("OPENAI_BASE_URL", "http://127.0.0.1:8787/v1"),
    api_key=os.environ["OPENAI_API_KEY"],
)
stream = client.chat.completions.create(
    model=os.getenv("HARNESS_MODEL", "codex/default"),
    messages=[{"role": "user", "content": "Explain reverse proxies in one sentence."}],
    stream=True,
)
for chunk in stream:
    if chunk.choices:
        print(chunk.choices[0].delta.content or "", end="", flush=True)
print()
