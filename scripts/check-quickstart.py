#!/usr/bin/env python3
"""Execute marked guide commands against a local POSIX harness fixture."""
import argparse
import json
import os
from pathlib import Path
import re
import shlex
import signal
import socket
import subprocess
import tempfile
import time

ROOT = Path(__file__).resolve().parents[1]
GUIDE = ROOT / 'docs/agent-quickstart.md'


def blocks(text):
    found = re.findall(r'<!-- quickstart-command: ([a-z]+) -->\s*```sh\n(.*?)```\s*<!-- /quickstart-command -->', text, re.S)
    if len(found) != 3 or {name for name, _ in found} != {'init', 'client', 'repair'}:
        raise ValueError('Expected exactly one marked init, client, and repair shell block')
    return dict(found)


def check(condition, message):
    if not condition:
        raise AssertionError(message)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--h2a', type=Path, required=True)
    parser.add_argument('--fixture', type=Path, required=True)
    parser.add_argument('--guide', type=Path, default=GUIDE, help='Guide to execute (for documentation drift tests)')
    args = parser.parse_args()
    check(os.name == 'posix', 'This checker requires POSIX process signals')
    commands = blocks(args.guide.read_text())
    with tempfile.TemporaryDirectory(prefix='h2a-guide-') as tmp:
        root = Path(tmp)
        bindir = root / 'bin'
        bindir.mkdir()
        (bindir / 'h2a').symlink_to(args.h2a.resolve())
        marker = root / 'calls'
        codex = bindir / 'codex'
        codex.write_text('#!/bin/sh\nif [ "$1" = "exec" ]; then echo call >> ' + shlex.quote(str(marker)) + '; fi\nexec ' + shlex.quote(str(args.fixture.resolve())) + ' "$@"\n')
        codex.chmod(0o700)
        with socket.socket() as sock:
            sock.bind(('127.0.0.1', 0))
            port = sock.getsockname()[1]
        state, events = root / 'state', root / 'events'
        env = dict(os.environ, PATH=str(bindir) + os.pathsep + os.environ.get('PATH', ''), H2A_STATE=str(state), H2A_EVENTS=str(events), H2A_PORT=str(port))
        env.pop('OPENAI_API_KEY', None)
        def run(name):
            return subprocess.run(['/bin/sh', '-c', commands[name]], env=env, text=True, capture_output=True, timeout=15)
        def start():
            events.unlink(missing_ok=True)
            return subprocess.Popen(['/bin/sh', '-c', commands['init']], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
        def wait_ready(proc):
            deadline = time.monotonic() + 12
            while time.monotonic() < deadline:
                if events.exists():
                    lines = events.read_text().splitlines()
                    for line in lines:
                        event = json.loads(line)
                        check(event['event'] != 'error', 'init emitted error before ready')
                        if event['event'] == 'ready':
                            return event
                check(proc.poll() is None, 'init exited before ready')
                time.sleep(.05)
            raise AssertionError('ready timed out')
        def stop(proc):
            proc.send_signal(signal.SIGINT)
            _, stderr = proc.communicate(timeout=10)
            check(proc.returncode == 0, 'foreground shutdown failed: ' + stderr)
        proc = None
        try:
            proc = start()
            ready = wait_ready(proc)
            token = Path(ready['credential_path']).read_text().strip()
            check(not marker.exists(), 'no-probe startup invoked fixture')
            client = run('client')
            check(client.returncode == 0 and 'Hello' in client.stdout, 'documented client failed: ' + client.stderr)
            check(token not in client.stdout + client.stderr, 'client printed credential')
            check(len(marker.read_text().splitlines()) == 1, 'client did not invoke exactly one completion')
            stop(proc)
            proc = None
            key_id = token.split('_')[1]
            def inspect(identifier):
                result = subprocess.run([str(args.h2a.resolve()), 'keys', 'inspect', '--state-dir', str(state), identifier], env=env, capture_output=True, text=True, timeout=10)
                check(result.returncode == 0, 'key inspection failed')
                return json.loads(result.stdout)
            original = inspect(key_id)
            check(original['accepted'] == 1, 'completion did not debit quota')
            proc = start()
            resumed = wait_ready(proc)
            check(Path(resumed['credential_path']).read_text().strip() == token and inspect(key_id) == original, 'restart changed credential or policy')
            stop(proc)
            proc = None
            other = subprocess.run([str(args.h2a.resolve()), 'keys', 'create', '--state-dir', str(state), '--name', 'unrelated', '--models', 'codex/*'], env=env, capture_output=True, text=True, timeout=10)
            check(other.returncode == 0, 'unrelated key creation failed')
            other_id = other.stdout.strip().split('_')[1]
            other_before = inspect(other_id)
            settings = state / 'client-settings.json'
            settings.write_text('{"unrelated": true}\n')
            settings_before = settings.read_bytes()
            # Simulate tomorrow in isolated stopped state, as in check-init.py.
            for path in (state / 'init/profile.json', state / 'keys' / (key_id + '.json')):
                data = json.loads(path.read_text())
                def expire(obj):
                    count = 0
                    if isinstance(obj, dict):
                        for k, v in obj.items():
                            if k == 'expires_at':
                                obj[k] = '2000-01-01T00:00:00Z'
                                count += 1
                            elif k == 'created_at':
                                obj[k] = '1999-12-31T23:00:00Z'
                            else:
                                count += expire(v)
                    elif isinstance(obj, list):
                        count += sum(expire(v) for v in obj)
                    return count
                check(expire(data) > 0, 'expiry field missing')
                path.write_text(json.dumps(data))
            before = (state / 'init/client.key').read_bytes()
            refusal = run('init')
            check(refusal.returncode == 3 and 'expir' in events.read_text().lower(), 'expired setup did not refuse')
            check((state / 'init/client.key').read_bytes() == before and inspect(other_id) == other_before, 'refusal mutated state')
            repair = run('repair')
            check(repair.returncode == 0 and not (state / 'init/client.key').exists(), 'documented repair failed')
            check(inspect(other_id) == other_before and settings.read_bytes() == settings_before, 'repair changed unrelated state')
            proc = start()
            new = wait_ready(proc)
            check(Path(new['credential_path']).read_text().strip() != token, 'new setup did not create a new key')
            check(inspect(other_id) == other_before and settings.read_bytes() == settings_before, 'new setup changed unrelated state')
            stop(proc)
            proc = None
            print('PASS: documented init, client, resume, expiry refusal, consented repair, and new setup (offline POSIX fixture)')
        finally:
            if proc is not None and proc.poll() is None:
                proc.send_signal(signal.SIGINT)
                try:
                    proc.communicate(timeout=5)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.communicate()


if __name__ == '__main__':
    main()
