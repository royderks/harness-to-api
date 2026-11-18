"""Real pinned clients against a local fake HTTP endpoint, not a gateway test."""
import importlib.util
import json
import os
from pathlib import Path
import subprocess
import sys
import threading
import unittest
from http.server import BaseHTTPRequestHandler, ThreadingHTTPServer

ROOT = Path(__file__).resolve().parents[2]


class Fixture(BaseHTTPRequestHandler):
    requests = []
    def log_message(self, *args):
        pass
    def do_POST(self):
        request = json.loads(self.rfile.read(int(self.headers['Content-Length'])))
        self.server.requests.append(request)
        first = not any(m['role'] == 'tool' for m in request['messages'])
        if not first:
            tool = next(m for m in request['messages'] if m['role'] == 'tool')
            assert tool['tool_call_id'] == 'call_fixture' and tool['content'] == 'operational'
        message = ({'role': 'assistant', 'content': None, 'tool_calls': [{'id': 'call_fixture', 'type': 'function', 'function': {'name': 'lookup_status', 'arguments': '{"service":"api"}'}}]}
                   if first else {'role': 'assistant', 'content': 'api is operational'})
        finish = 'tool_calls' if first else 'stop'
        if getattr(self.server, 'omit_finish', None) == ('first' if first else 'second'):
            finish = None
        self.send_response(200)
        self.send_header('Content-Type', 'text/event-stream' if request.get('stream') else 'application/json')
        self.end_headers()
        base = {'id': 'chatcmpl-fixture', 'created': 1, 'model': 'codex/default'}
        if request.get('stream'):
            delta = ({'role': 'assistant', 'tool_calls': [{'index': 0, **message['tool_calls'][0]}]} if first else message)
            for part in ({'delta': delta, 'finish_reason': None}, {'delta': {}, 'finish_reason': finish}):
                chunk = {**base, 'object': 'chat.completion.chunk', 'choices': [{'index': 0, **part}]}
                self.wfile.write(('data: ' + json.dumps(chunk) + '\n\n').encode())
            self.wfile.write(b'data: [DONE]\n\n')
        else:
            self.wfile.write(json.dumps({**base, 'object': 'chat.completion', 'choices': [{'index': 0, 'message': message, 'finish_reason': finish}]}).encode())


class ToolCalling(unittest.TestCase):
    def test_sdk_roundtrips(self):
        server = ThreadingHTTPServer(('127.0.0.1', 0), Fixture)
        server.requests = []
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        self.addCleanup(server.server_close)
        self.addCleanup(server.shutdown)
        env = dict(os.environ, OPENAI_BASE_URL=f'http://127.0.0.1:{server.server_port}/v1', OPENAI_API_KEY='fixture-key', H2A_MODEL='codex/default', OPENAI_AGENTS_DISABLE_TRACING='1')
        for script, stream in [('main.py', False), ('main.py', True), ('agent.py', False)]:
            with self.subTest(script=script, stream=stream):
                before = len(server.requests)
                argv = [sys.executable, str(ROOT / 'examples/tool-calling' / script)] + (['--stream'] if stream else [])
                proc = subprocess.run(argv, env=env, capture_output=True, text=True, timeout=30)
                self.assertEqual(proc.returncode, 0, proc.stderr)
                self.assertIn('operational', proc.stdout)
                self.assertEqual(len(server.requests) - before, 2)
                first, second = server.requests[before:]
                self.assertEqual(first['tools'][0]['function']['name'], 'lookup_status')
                self.assertTrue(first['tools'][0]['function']['strict'])
                self.assertEqual(next(m for m in second['messages'] if m['role'] == 'tool')['tool_call_id'], 'call_fixture')

    def test_agents_reject_incomplete_turn_before_continuing(self):
        for omitted, count in [('first', 1), ('second', 2)]:
            with self.subTest(omitted=omitted):
                server = ThreadingHTTPServer(('127.0.0.1', 0), Fixture)
                server.requests = []
                server.omit_finish = omitted
                thread = threading.Thread(target=server.serve_forever, daemon=True)
                thread.start()
                try:
                    env = dict(os.environ, OPENAI_BASE_URL=f'http://127.0.0.1:{server.server_port}/v1', OPENAI_API_KEY='fixture-key', H2A_MODEL='codex/default', OPENAI_AGENTS_DISABLE_TRACING='1')
                    proc = subprocess.run([sys.executable, str(ROOT / 'examples/tool-calling/agent.py')], env=env, capture_output=True, text=True, timeout=30)
                    self.assertNotEqual(proc.returncode, 0)
                    self.assertNotIn('operational', proc.stdout)
                    self.assertEqual(len(server.requests), count)
                finally:
                    server.shutdown()
                    server.server_close()
                    thread.join()

    def test_incomplete_stream_and_bad_arguments_rejected(self):
        spec = importlib.util.spec_from_file_location('tool_example', ROOT / 'examples/tool-calling/main.py')
        module = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(module)
        for arguments in ('{}', '{"service":"api","service":"api"}', '{"service":"other"}', '{"service":"api","extra":1}'):
            with self.subTest(arguments=arguments), self.assertRaises(ValueError):
                module.execute('lookup_status', arguments)
        with self.assertRaises(ValueError):
            module.execute('unexpected', '{"service":"api"}')
        with self.assertRaises(ValueError):
            module.streamed([])
        from types import SimpleNamespace as N
        def chunk(call=None, finish=None):
            return N(choices=[N(index=0, finish_reason=finish, delta=N(content=None, tool_calls=[call] if call else []))])
        def fragment(id=None, kind=None, name=None, arguments=None):
            return N(index=0, id=id, type=kind, function=N(name=name, arguments=arguments))
        fragments = [chunk(fragment('call_', 'function', 'lookup_', '{\"service\":')),
                     chunk(fragment('fixture', None, 'status', '\"api\"}'))]
        with self.assertRaises(ValueError):
            module.streamed(fragments)
        finish, _, calls = module.streamed(fragments + [chunk(finish='tool_calls')])
        self.assertEqual((finish, calls[0]['id'], calls[0]['function']['name']), ('tool_calls', 'call_fixture', 'lookup_status'))
        self.assertEqual(module.execute(calls[0]['function']['name'], calls[0]['function']['arguments']), 'operational')

if __name__ == '__main__':
    unittest.main()
