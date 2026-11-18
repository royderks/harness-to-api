#!/usr/bin/env python3
"""Run pinned clients through a local h2a and tool-calling harness fixture."""
import json
import os
from pathlib import Path
import shlex
import signal
import socket
import subprocess
import sys
import tempfile
import time
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
BIN = Path(os.environ.get('H2A_BIN', ROOT / 'target/debug/h2a')).resolve()
FIXTURE = Path(os.environ.get('H2A_FIXTURE', ROOT / 'target/debug/examples/harness-fixture')).resolve()


def main():
    print('OFFLINE FIXTURE — no provider calls', flush=True)
    with tempfile.TemporaryDirectory(prefix='h2a-tools-') as tmp:
        state = Path(tmp) / 'state'
        def cli(*args):
            result = subprocess.run([str(BIN), *args[:2], '--state-dir', str(state), *args[2:]], capture_output=True, text=True, timeout=15)
            if result.returncode:
                raise RuntimeError('h2a command failed: ' + ' '.join(args[:2]))
            return result.stdout.strip()
        token = cli('keys', 'create', '--name', 'tools-fixture', '--models', 'codex/default', '--ttl', '10m', '--max-requests', '8', '--request-timeout', '20s', '--max-concurrent', '1')
        identifier = token.split('_')[1]
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        base = f'http://127.0.0.1:{port}'
        with open(Path(tmp) / 'server.log', 'w') as log:
            server = subprocess.Popen([str(BIN), 'serve', '--state-dir', str(state), '--provider', 'codex', '--port', str(port), '--codex-bin', str(FIXTURE)], stdout=log, stderr=log)
            try:
                for _ in range(100):
                    if server.poll() is not None:
                        raise RuntimeError('gateway startup failed')
                    try:
                        with urllib.request.urlopen(base + '/healthz', timeout=.2):
                            break
                    except OSError:
                        time.sleep(.05)
                else:
                    raise RuntimeError('gateway startup timed out')
                env = dict(os.environ, OPENAI_BASE_URL=base + '/v1', OPENAI_API_KEY=token, H2A_MODEL='codex/default', OPENAI_AGENTS_DISABLE_TRACING='1')
                commands = json.loads((ROOT / 'examples/commands.json').read_text())['tool-calling']['commands']
                if len(commands) != 4 or len(set(commands)) != 4:
                    raise RuntimeError('Expected four documented tool-loop commands')
                for index, command in enumerate(commands, 1):
                    parts = shlex.split(command)
                    if parts[0] != 'python' or parts[1] not in ('examples/tool-calling/main.py', 'examples/tool-calling/agent.py') or parts[2:] not in ([], ['--stream']):
                        raise RuntimeError('Invalid documented tool-loop command')
                    script, flags = Path(parts[1]).name, parts[2:]
                    result = subprocess.run([sys.executable, str(ROOT / parts[1]), *flags], env=env, capture_output=True, text=True, timeout=90)
                    if token in result.stdout + result.stderr or result.returncode or 'operational' not in result.stdout.lower():
                        raise RuntimeError('Client failed: ' + script + ' ' + ' '.join(flags))
                    snapshot = json.loads(cli('keys', 'inspect', identifier))
                    if snapshot['accepted'] != 2 * index:
                        raise RuntimeError('Unexpected accepted request count')
                    print('PASS ' + script + ' ' + ' '.join(flags) + ': two accepted requests', flush=True)
                def denied(expected):
                    result = subprocess.run([sys.executable, str(ROOT / 'examples/tool-calling/main.py')], env=env, capture_output=True, text=True, timeout=15)
                    if result.returncode != 1 or expected not in result.stderr or token in result.stdout + result.stderr:
                        raise RuntimeError('Denial check failed')
                denied('429')
                cli('keys', 'revoke', identifier)
                denied('401')
                print('PASS quota and revocation', flush=True)
            finally:
                server.send_signal(signal.SIGINT)
                try:
                    server.wait(timeout=10)
                except subprocess.TimeoutExpired:
                    server.kill()
                    server.wait()
    print('PASS temporary gateway and key cleanup')

if __name__ == '__main__':
    main()
