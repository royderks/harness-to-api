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
        token = cli('keys', 'create', '--name', 'tools-fixture', '--models', 'codex/default', '--ttl', '10m', '--max-requests', '6', '--request-timeout', '20s', '--max-concurrent', '1')
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
                if len(commands) != 3 or len(set(commands)) != 3:
                    raise RuntimeError('Expected three documented tool-loop commands')
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
                probe_token = cli('keys', 'create', '--name', 'tool-probe-fixture', '--models', 'codex/default', '--max-requests', '1', '--request-timeout', '20s')
                probe_id = probe_token.split('_')[1]
                probe = subprocess.run([str(BIN), 'probe', '--base-url', base + '/v1', '--model', 'codex/default', '--tool-call'],
                                       env=dict(env, OPENAI_API_KEY=probe_token), capture_output=True, text=True, timeout=30)
                if probe.returncode or probe_token in probe.stdout + probe.stderr:
                    raise RuntimeError('Tool-call CLI probe failed')
                result = json.loads(probe.stdout)
                if result.get('available') is not True or result.get('capability') != 'tool_call':
                    raise RuntimeError('Tool-call CLI probe did not verify capability')
                if json.loads(cli('keys', 'inspect', probe_id))['accepted'] != 1:
                    raise RuntimeError('Probe did not consume exactly one request')
                cli('keys', 'revoke', probe_id)
                print('PASS explicit CLI tool-call probe: one request, no function execution', flush=True)
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
