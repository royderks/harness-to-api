"""Agents SDK executes the same local function through Chat Completions."""
import argparse
import asyncio
import os
import sys
from pathlib import Path
os.environ["OPENAI_AGENTS_DISABLE_TRACING"] = "1"
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from _client import config, require_text, run
from agents import Agent, Runner, OpenAIChatCompletionsModel, function_tool, set_tracing_disabled
import httpx
from openai import AsyncOpenAI

async def demo(stream=False):
    calls = 0

    @function_tool
    async def lookup_status(service: str) -> str:
        """Look up the status of a local service."""
        nonlocal calls
        calls += 1
        if calls != 1 or service != "api":
            raise ValueError("Expected one lookup of the api service")
        return "operational"

    set_tracing_disabled(True)
    base, token, model = config()
    async with AsyncOpenAI(base_url=base, api_key=token, max_retries=0, timeout=150,
                           http_client=httpx.AsyncClient(follow_redirects=False, trust_env=False)) as client:
        agent = Agent(name="Service status", instructions="Call lookup_status for service api, then report its result.",
                      model=OpenAIChatCompletionsModel(model=model, openai_client=client), tools=[lookup_status])
        if stream:
            result = Runner.run_streamed(agent, "Check the api service.", max_turns=2)
            async for _ in result.stream_events():
                pass
        else:
            result = await Runner.run(agent, "Check the api service.", max_turns=2)
        if calls != 1:
            raise ValueError("Expected exactly one local function call")
        print(require_text(result.final_output))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--stream", action="store_true")
    args = parser.parse_args()
    run(lambda: asyncio.run(demo(args.stream)))
