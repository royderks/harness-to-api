"""A fixed draft → review graph; no model-selected tools or hidden retries."""
import os
import sys
from pathlib import Path
os.environ["LANGSMITH_TRACING"] = "false"
os.environ["LANGCHAIN_TRACING_V2"] = "false"
sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from _client import PROMPT, config, require_text, run, sdk_client
from langgraph.graph import StateGraph, START, END
from typing_extensions import TypedDict


class State(TypedDict):
    draft: str
    review: str


def main():
    _, _, model = config()
    reviewer = os.environ.get("H2A_REVIEW_MODEL", model)
    with sdk_client() as client:
        def ask(selected, prompt):
            return require_text(client.chat.completions.create(model=selected, messages=[{"role": "user", "content": prompt}]).choices[0].message.content)

        def draft(state):
            text = ask(model, PROMPT)
            print("DRAFT (" + model + ")\n" + text, flush=True)
            return {"draft": text}

        def review(state):
            text = ask(reviewer, "Review this incident update for invented facts and clarity. Return a corrected three-bullet update. Original task: " + PROMPT + "\nDraft:\n" + state["draft"])
            print("\nREVIEW (" + reviewer + ")\n" + text, flush=True)
            return {"review": text}

        graph = StateGraph(State)
        graph.add_node("draft_update", draft)
        graph.add_node("review_update", review)
        graph.add_edge(START, "draft_update")
        graph.add_edge("draft_update", "review_update")
        graph.add_edge("review_update", END)
        graph.compile().invoke({"draft": "", "review": ""})


if __name__ == "__main__":
    run(main)
