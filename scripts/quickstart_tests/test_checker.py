"""Verify the guide checker rejects a broken documented client."""
import http.server
import json
import os
import re
from pathlib import Path
import subprocess
import sys
import tempfile
import threading
import unittest

ROOT = Path(__file__).resolve().parents[2]


class QuickstartCheckerTests(unittest.TestCase):
    def test_client_rejects_bad_completions(self):
        guide = (ROOT / 'docs/agent-quickstart.md').read_text()
        command = re.search(r'<!-- quickstart-command: client -->\s*```sh\n(.*?)```', guide, re.S).group(1)
        class Handler(http.server.BaseHTTPRequestHandler):
            def do_POST(self):
                self.rfile.read(int(self.headers['Content-Length']))
                body = json.dumps(self.server.answer).encode()
                self.send_response(200)
                self.send_header('Content-Length', str(len(body)))
                self.end_headers()
                self.wfile.write(body)
            def log_message(self, *args):
                pass
        server = http.server.HTTPServer(('127.0.0.1', 0), Handler)
        thread = threading.Thread(target=server.serve_forever, daemon=True)
        thread.start()
        try:
            with tempfile.TemporaryDirectory() as tmp:
                root = Path(tmp)
                credential = root / 'key'
                credential.write_text('fixture-secret')
                events = root / 'events'
                events.write_text(json.dumps({'event': 'ready', 'credential_path': str(credential), 'model': 'codex/default', 'endpoint': 'http://127.0.0.1:%s/v1' % server.server_port}))
                env = dict(os.environ, H2A_EVENTS=str(events))
                for answer in ({'error': {'message': 'failed'}}, {'choices': []},
                               {'choices': [{'finish_reason': 'stop', 'message': {'content': '  '}}]},
                               {'choices': [{'finish_reason': None, 'message': {'content': 'unfinished'}}]}):
                    with self.subTest(answer=answer):
                        server.answer = answer
                        result = subprocess.run(['/bin/sh', '-c', command], env=env, capture_output=True, text=True, timeout=10)
                        self.assertNotEqual(result.returncode, 0)
                        self.assertNotIn('fixture-secret', result.stdout + result.stderr)
        finally:
            server.shutdown()
            thread.join()
            server.server_close()

    def test_broken_documented_endpoint_fails(self):
        binary = os.environ.get('H2A_BIN')
        fixture = os.environ.get('H2A_FIXTURE')
        if not binary or not fixture:
            self.skipTest('set H2A_BIN and H2A_FIXTURE to run black-box drift test')
        cases = [
            ('docs/agent-quickstart.md', '--guide', "ready['endpoint'] + '/chat/completions'", "ready['endpoint'] + '/broken'", 'documented client failed'),
            ('docs/getting-started.md', '--human-guide', 'http://127.0.0.1:8787/v1/chat/completions', 'http://127.0.0.1:8787/v1/broken', 'documented human client failed'),
        ]
        for filename, flag, original, replacement, error in cases:
            with self.subTest(guide=filename):
                guide = (ROOT / filename).read_text()
                command = re.search(r'<!-- quickstart-command: client -->\s*```sh\n(.*?)```', guide, re.S).group(1)
                self.assertEqual(command.count(original), 1)
                self.assertEqual(guide.count(command), 1)
                with tempfile.TemporaryDirectory() as tmp:
                    broken = Path(tmp) / 'broken.md'
                    broken.write_text(guide.replace(command, command.replace(original, replacement)))
                    result = subprocess.run([sys.executable, str(ROOT / 'scripts/check-quickstart.py'), '--h2a', binary, '--fixture', fixture, flag, str(broken)], capture_output=True, text=True, timeout=30)
                self.assertNotEqual(result.returncode, 0)
                self.assertIn(error, result.stderr)


if __name__ == '__main__':
    unittest.main()
