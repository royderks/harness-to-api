"""Agents SDK executes the same local function through Chat Completions."""
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

async def require_completed_turn(response):
    if response.status_code != 200:
        return  # The SDK handles ordinary HTTP errors.
    await response.aread()
    body = response.json()
    choices = body.get("choices", [])
    if len(choices) != 1:
        raise ValueError("Expected one completed choice")
    choice = choices[0]
    message = choice.get("message", {})
    calls = message.get("tool_calls")
    if message.get("role") != "assistant" or choice.get("finish_reason") != ("tool_calls" if calls else "stop"):
        raise ValueError("Incomplete model turn")
    if calls and message.get("content") is not None:
        raise ValueError("Conflicting tool-call content")


async def demo():
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
                           http_client=httpx.AsyncClient(follow_redirects=False, trust_env=False,
                                                         event_hooks={"response": [require_completed_turn]})) as client:
        agent = Agent(name="Service status", instructions="Call lookup_status for service api, then report its result.",
                      model=OpenAIChatCompletionsModel(model=model, openai_client=client), tools=[lookup_status])
        result = await Runner.run(agent, "Check the api service.", max_turns=2)
        if calls != 1:
            raise ValueError("Expected exactly one local function call")
        print(require_text(result.final_output))


if __name__ == "__main__":
    run(lambda: asyncio.run(demo()))
