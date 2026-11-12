"""Text-only agent using Chat Completions; no OpenAI account or tracing export."""
import asyncio
import os
import sys
from pathlib import Path
os.environ["OPENAI_AGENTS_DISABLE_TRACING"] = "1"
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from _client import PROMPT, config, require_text, run
import httpx
from openai import AsyncOpenAI
from agents import Agent, Runner, OpenAIChatCompletionsModel, ModelSettings, set_tracing_disabled


async def demo():
    set_tracing_disabled(True)
    base, token, model = config()
    async with AsyncOpenAI(base_url=base, api_key=token, max_retries=0, timeout=150,
                           http_client=httpx.AsyncClient(follow_redirects=False, trust_env=False)) as client:
        agent = Agent(name="Incident editor", instructions="Write concise incident updates without inventing facts.",
                      model=OpenAIChatCompletionsModel(model=model, openai_client=client),
                      model_settings=ModelSettings(), tools=[])
        result = await Runner.run(agent, PROMPT, max_turns=1)
        print(require_text(result.final_output))


if __name__ == "__main__":
    run(lambda: asyncio.run(demo()))
