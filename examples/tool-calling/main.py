"""Two bounded Chat Completions turns with a client-owned function."""
import argparse
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from _client import config, require_text, run, sdk_client

PROMPT = "Call lookup_status for service api, then report the result."
TOOLS = [{"type": "function", "function": {"name": "lookup_status", "description": "Look up a local service status", "strict": True, "parameters": {"type": "object", "properties": {"service": {"type": "string"}}, "required": ["service"], "additionalProperties": False}}}]
REGISTRY = {"api": "operational"}


def lookup_status(service: str) -> str:
    if not isinstance(service, str) or service not in REGISTRY:
        raise ValueError("Unknown service")
    return REGISTRY[service]


def execute(name, arguments):
    if name != "lookup_status":
        raise ValueError("Unexpected function")
    def pairs(items):
        result = {}
        for key, value in items:
            if key in result:
                raise ValueError("Duplicate argument")
            result[key] = value
        return result
    args = json.loads(arguments, object_pairs_hook=pairs)
    if not isinstance(args, dict) or set(args) != {"service"}:
        raise ValueError("Invalid arguments")
    return lookup_status(args["service"])


def streamed(response):
    calls = {}
    content = []
    finish = None
    for chunk in response:
        for choice in chunk.choices:
            if choice.index != 0 or finish is not None:
                raise ValueError("Unexpected stream choice")
            delta = choice.delta
            if delta.content:
                content.append(delta.content)
            for call in delta.tool_calls or []:
                state = calls.setdefault(call.index, {"id": "", "type": None, "name": "", "arguments": ""})
                if call.id:
                    state["id"] += call.id
                if call.type:
                    if state["type"] is not None:
                        raise ValueError("Duplicate call type")
                    state["type"] = call.type
                if call.function:
                    state["name"] += call.function.name or ""
                    state["arguments"] += call.function.arguments or ""
            if choice.finish_reason:
                finish = choice.finish_reason
    if finish not in ("tool_calls", "stop"):
        raise ValueError("Incomplete stream")
    if finish == "tool_calls":
        if content or sorted(calls) != [0]:
            raise ValueError("Unexpected streamed calls")
        call = calls[0]
        if not call["id"] or call["type"] != "function" or not call["name"] or not call["arguments"]:
            raise ValueError("Incomplete streamed call")
        return finish, None, [{"id": call["id"], "type": "function", "function": {"name": call["name"], "arguments": call["arguments"]}}]
    if calls:
        raise ValueError("Unexpected calls on final turn")
    return finish, require_text("".join(content)), []


def turn(client, model, messages, stream):
    response = client.chat.completions.create(model=model, messages=messages, tools=TOOLS,
                                               parallel_tool_calls=False, stream=stream)
    if stream:
        return streamed(response)
    if len(response.choices) != 1:
        raise ValueError("Unexpected completion choices")
    choice = response.choices[0]
    calls = [call.model_dump(exclude_none=True) for call in choice.message.tool_calls or []]
    return choice.finish_reason, choice.message.content, calls


def demo(stream=False):
    _, _, model = config()
    messages = [{"role": "user", "content": PROMPT}]
    with sdk_client() as client:
        finish, content, calls = turn(client, model, messages, stream)
        if finish != "tool_calls" or content is not None or len(calls) != 1:
            raise ValueError("Expected one function call")
        call = calls[0]
        if call.get("type") != "function" or not call.get("id"):
            raise ValueError("Invalid function call")
        result = execute(call["function"]["name"], call["function"]["arguments"])
        messages.extend([{"role": "assistant", "content": None, "tool_calls": calls},
                         {"role": "tool", "tool_call_id": call["id"], "content": result}])
        finish, content, calls = turn(client, model, messages, stream)
        if finish != "stop" or calls:
            raise ValueError("Expected final text")
        print(require_text(content))


if __name__ == "__main__":
    parser = argparse.ArgumentParser()
    parser.add_argument("--stream", action="store_true")
    args = parser.parse_args()
    run(lambda: demo(args.stream))
