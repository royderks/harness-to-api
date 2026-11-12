"""Exercise empty completions through the documented example entry points."""
import asyncio
import contextlib
import importlib.util
import io
import json
import os
from pathlib import Path
import sys
from types import SimpleNamespace
import unittest
from unittest.mock import AsyncMock, MagicMock, Mock, patch

ROOT = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(ROOT))
import _client


def load(name):
    spec = importlib.util.spec_from_file_location('completion_' + name, ROOT / name / 'main.py')
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


class CompletionValidation(unittest.TestCase):
    def setUp(self):
        env = patch.dict(os.environ, OPENAI_BASE_URL='https://example.invalid/v1',
                         OPENAI_API_KEY='test-token', H2A_MODEL='codex/default',
                         H2A_MODELS='codex/default')
        env.start()
        self.addCleanup(env.stop)
        output = contextlib.redirect_stdout(io.StringIO())
        output.__enter__()
        self.addCleanup(output.__exit__, None, None, None)

    def test_http_client_rejects_empty_content(self):
        for content in (None, '', ' \n'):
            response = io.BytesIO(json.dumps({'choices': [{'message': {'content': content}}]}).encode())
            with patch.object(_client.urllib.request, 'build_opener') as opener:
                opener.return_value.open.return_value = response
                with self.assertRaises(ValueError):
                    _client.complete()

    def test_stream_heading_does_not_count_as_completion(self):
        module = load('harness-switch')
        for content in (None, '', ' \n'):
            client, stream = MagicMock(), MagicMock()
            stream.__enter__.return_value = [SimpleNamespace(choices=[SimpleNamespace(delta=SimpleNamespace(content=content), finish_reason="stop")])]
            client.chat.completions.create.return_value = stream
            with patch.object(module, 'sdk_client') as factory:
                factory.return_value.__enter__.return_value = client
                with self.assertRaises(ValueError):
                    module.main()

    def test_real_sdk_requires_terminal_stop_event(self):
        import httpx
        from openai import OpenAI
        module = load('harness-switch')
        def event(content, finish):
            return 'data: ' + json.dumps({
                'id': 'chatcmpl-test', 'object': 'chat.completion.chunk',
                'created': 1, 'model': 'codex/default',
                'choices': [{'index': 0, 'delta': {'content': content}, 'finish_reason': finish}]
            }) + '\n\n'
        partial = event('partial answer', None)
        for ending, succeeds in (('', False), ('data: [DONE]\n\n', False),
                                 (event('', 'length'), False),
                                 (event('', 'content_filter'), False),
                                 (event('', 'stop') + 'data: [DONE]\n\n', True)):
            with self.subTest(ending=ending):
                transport = httpx.MockTransport(lambda req: httpx.Response(
                    200, headers={'content-type': 'text/event-stream'}, content=partial + ending))
                client = OpenAI(base_url='https://example.invalid/v1', api_key='test-token',
                                http_client=httpx.Client(transport=transport))
                with patch.object(module, 'sdk_client', return_value=client):
                    if succeeds:
                        module.main()
                    else:
                        with self.assertRaisesRegex(RuntimeError, 'Stream did not finish'):
                            module.main()

    def test_graph_requires_text_from_both_nodes(self):
        module = load('langgraph')
        for responses in (['', 'review'], ['draft', ' ']):
            client = MagicMock()
            client.chat.completions.create.side_effect = [SimpleNamespace(choices=[SimpleNamespace(message=SimpleNamespace(content=text))]) for text in responses]
            with patch.object(module, 'sdk_client') as factory:
                factory.return_value.__enter__.return_value = client
                with self.assertRaises(ValueError):
                    module.main()

    def test_agent_requires_final_text(self):
        module = load('agents-sdk')
        for text in (None, '', ' '):
            with patch.object(module, 'AsyncOpenAI', return_value=MagicMock()), \
                 patch.object(module.Runner, 'run', new=AsyncMock(return_value=SimpleNamespace(final_output=text))):
                with self.assertRaises(ValueError):
                    asyncio.run(module.demo())

    def test_e2b_status_messages_do_not_count_and_cleanup_runs(self):
        module = load('e2b')
        sandbox = Mock()
        sandbox.commands.run.return_value = SimpleNamespace(exit_code=0, stdout=' \n')
        with patch.object(module.Sandbox, 'create', return_value=sandbox):
            with self.assertRaises(ValueError):
                module.main()
        sandbox.kill.assert_called_once()

    def test_modal_status_messages_do_not_count_and_cleanup_runs(self):
        module = load('modal')
        sandbox = Mock(returncode=0)
        sandbox.stdout.read.return_value = ''
        with patch.object(module.modal, 'App', return_value=MagicMock()), \
             patch.object(module.modal.Image, 'debian_slim'), \
             patch.object(module.modal.Secret, 'from_dict'), \
             patch.object(module.modal.Sandbox, 'create', return_value=sandbox):
            with self.assertRaises(ValueError):
                module.main()
        sandbox.terminate.assert_called_once()


if __name__ == '__main__':
    unittest.main()
