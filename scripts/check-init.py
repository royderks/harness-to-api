#!/usr/bin/env python3
"""Offline black-box init proof; requires a built h2a and a Go toolchain (POSIX)."""
import concurrent.futures
import json
import os
from pathlib import Path
import queue
import shlex
import shutil
import signal
import socket
import subprocess
import tempfile
import threading
import time
import urllib.error
import urllib.request

ROOT = Path(__file__).resolve().parents[1]
BIN = str(Path(os.environ.get('H2A_BIN', ROOT / 'bin/h2a')).resolve())
GO = os.environ.get('GO', 'go')


def check(value, message):
    if not value:
        raise AssertionError(message)


def main():
    check(os.name == 'posix', 'This process/signal proof requires POSIX')
    with tempfile.TemporaryDirectory(prefix='h2a-init-proof-') as tmp:
        root = Path(tmp)
        fixture, marker = root / 'fake-cli', root / 'calls'
        subprocess.run([GO, 'build', '-o', str(fixture), './internal/harness/testdata/fake-cli'], cwd=ROOT, check=True)
        wrapper = root / 'codex'
        wrapper.write_text('#!/bin/sh\nprintf "%s\\n" "$$" >> ' + shlex.quote(str(marker)) + '\nexec ' + shlex.quote(str(fixture)) + ' "$@"\n')
        wrapper.chmod(0o700)
        env = dict(os.environ, PATH=str(root) + os.pathsep + os.environ.get('PATH', ''))
        env.pop('OPENAI_API_KEY', None)
        processes, streams, secrets = [], [], []

        def port():
            with socket.socket() as sock:
                sock.bind(('127.0.0.1', 0))
                return sock.getsockname()[1]

        def args(state, number, probe='--no-probe', model='codex/default'):
            return ['init', '--provider', 'codex', '--model', model, '--state-dir', str(state), '--port', str(number), probe, '--json']

        def run(*argv):
            result = subprocess.run([BIN, *argv], env=env, text=True, capture_output=True, timeout=12)
            streams.extend([result.stdout, result.stderr])
            return result

        def start(state, number, probe='--no-probe', model='codex/default'):
            started = time.monotonic()
            proc = subprocess.Popen([BIN, *args(state, number, probe, model)], env=env, text=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE)
            processes.append(proc)
            lines = queue.Queue()
            output, errors = [], []
            def read_stdout():
                for line in proc.stdout:
                    output.append(line)
                    lines.put(line)
            def read_stderr():
                errors.append(proc.stderr.read())
            threads = [threading.Thread(target=read_stdout, daemon=True), threading.Thread(target=read_stderr, daemon=True)]
            for thread in threads:
                thread.start()
            proc.proof = (lines, output, errors, threads, started)
            return proc

        def event(proc, wanted):
            lines, _, _, _, _ = proc.proof
            deadline = time.monotonic() + 12
            while time.monotonic() < deadline:
                try:
                    line = lines.get(timeout=.1)
                except queue.Empty:
                    check(proc.poll() is None, 'init exited before ' + wanted)
                    continue
                obj = json.loads(line)
                check(obj.get('schema_version') == 1, 'wrong NDJSON version')
                if obj.get('event') == wanted:
                    return obj
                check(obj.get('event') != 'error', 'init emitted error before ' + wanted)
            raise AssertionError('timed out waiting for ' + wanted)

        def stop(proc):
            if proc.poll() is None:
                proc.send_signal(signal.SIGINT)
                try:
                    proc.wait(timeout=8)
                except subprocess.TimeoutExpired:
                    proc.kill()
                    proc.wait()
                    raise AssertionError('init failed to stop after SIGINT')
            _, output, errors, threads, _ = proc.proof
            for thread in threads:
                thread.join(timeout=3)
            streams.extend([''.join(output), ''.join(errors)])
            for line in output:
                obj = json.loads(line)
                check(obj.get('schema_version') == 1, 'invalid NDJSON version')
            return proc.returncode

        def credential(state):
            path = state / 'init' / 'client.key'
            check(path.is_file() and not path.is_symlink(), 'credential is not a regular file')
            check(path.stat().st_mode & 0o077 == 0, 'credential permissions are too broad')
            token = path.read_text().strip()
            check(token.startswith('h2a_'), 'invalid credential')
            secrets.append(token)
            return token, token.split('_')[1]

        def inspect(state, identifier):
            result = run('keys', 'inspect', '--state-dir', str(state), identifier)
            check(result.returncode == 0, 'key inspection failed')
            return json.loads(result.stdout)

        def request(number, token, model='codex/default'):
            body = json.dumps({'model': model, 'messages': [{'role': 'user', 'content': 'Offline fixture.'}]}).encode()
            req = urllib.request.Request(f'http://127.0.0.1:{number}/v1/chat/completions', data=body, headers={'Authorization': 'Bearer ' + token, 'Content-Type': 'application/json'})
            try:
                with urllib.request.urlopen(req, timeout=10) as response:
                    return response.status
            except urllib.error.HTTPError as error:
                return error.code
            except (urllib.error.URLError, OSError):
                return None

        try:
            state, number = root / 'main', port()
            proc = start(state, number)
            event(proc, 'plan')
            event(proc, 'ready')
            elapsed = time.monotonic() - proc.proof[4]
            token, identifier = credential(state)
            check(not marker.exists(), 'startup invoked inference without consent')
            before = inspect(state, identifier)
            check(before['accepted'] == 0, 'startup consumed quota')
            check(request(number, token) == 200, 'authenticated fixture request failed')
            after = inspect(state, identifier)
            check(after['accepted'] == 1, 'request did not debit quota')
            second = run(*args(state, number))
            check(second.returncode == 3, 'second owner did not refuse')
            check(request(number, token) == 200, 'first owner stopped serving')
            after = inspect(state, identifier)
            check(stop(proc) == 0, 'normal stop failed')
            proc = start(state, number)
            event(proc, 'ready')
            token2, identifier2 = credential(state)
            resumed = inspect(state, identifier2)
            check((token2, identifier2) == (token, identifier), 'repeat replaced credential')
            check(resumed['accepted'] == after['accepted'] and resumed['policy']['expires_at'] == before['policy']['expires_at'], 'repeat reset usage or expiry')
            check(stop(proc) == 0, 'repeat stop failed')
            print(f'PASS: clean launch, ownership and repeat; one offline fixture sample launch-to-ready {elapsed:.3f}s ({os.uname().sysname} {os.uname().machine}, n=1)')

            occupied = socket.socket()
            occupied.bind(('127.0.0.1', 0))
            occupied.listen()
            conflict_state = root / 'conflict'
            conflict = run(*args(conflict_state, occupied.getsockname()[1]))
            check(conflict.returncode == 3, 'occupied port was not refused')
            with socket.create_connection(occupied.getsockname(), timeout=2):
                pass
            check(not (conflict_state / 'init' / 'profile.json').exists() and not (conflict_state / 'init' / 'client.key').exists(), 'port conflict published setup')
            occupied.close()
            print('PASS: unrelated port listener survives without published setup')

            probe_state, probe_port = root / 'probe', port()
            calls = len(marker.read_text().splitlines()) if marker.exists() else 0
            probe = start(probe_state, probe_port, '--probe')
            event(probe, 'probe')
            probe_token, probe_id = credential(probe_state)
            check(len(marker.read_text().splitlines()) == calls + 1, 'probe did not invoke exactly one fixture request')
            check(inspect(probe_state, probe_id)['accepted'] == 1, 'probe did not debit quota')
            check(stop(probe) == 0, 'probe process failed to stop')
            print('PASS: explicit probe makes one quota-counted fixture request')
            failed_probe = run(*args(root / 'failed-probe', port(), '--probe', 'codex/exit-error'))
            check(failed_probe.returncode == 4, 'failed probe did not return probe-failure status')
            check('sensitive credentials' not in failed_probe.stdout + failed_probe.stderr, 'raw provider diagnostics leaked')
            print('PASS: failed probe is sanitized and exits with probe failure')

            # Edit isolated stopped fixture state to exercise otherwise hour-long policy boundaries.
            exhausted_state = root / 'exhausted'
            shutil.copytree(state, exhausted_state)
            record_path = exhausted_state / 'keys' / (identifier + '.json')
            record = json.loads(record_path.read_text())
            record['usage']['accepted'] = 20
            record_path.write_text(json.dumps(record))
            refused = run(*args(exhausted_state, number))
            check(refused.returncode == 3 and 'exhaust' in (refused.stdout + refused.stderr).lower(), 'exhausted setup did not require repair')

            expired_state = root / 'expired'
            shutil.copytree(state, expired_state)
            profile_path = expired_state / 'init' / 'profile.json'
            expired_record = expired_state / 'keys' / (identifier + '.json')
            def expire(obj):
                count = 0
                if isinstance(obj, dict):
                    for key, value in obj.items():
                        if key == 'expires_at':
                            obj[key] = '2000-01-01T00:00:00Z'
                            count += 1
                        else:
                            count += expire(value)
                elif isinstance(obj, list):
                    for item in obj:
                        count += expire(item)
                return count
            for path in (profile_path, expired_record):
                data = json.loads(path.read_text())
                check(expire(data) > 0, 'fixture expiry field not found')
                path.write_text(json.dumps(data))
            refused = run(*args(expired_state, number))
            check(refused.returncode == 3 and 'expir' in (refused.stdout + refused.stderr).lower(), 'expired setup did not require repair')
            print('PASS: exhausted and expired isolated setups require repair')

            hang_state, hang_port = root / 'hang', port()
            previous_calls = len(marker.read_text().splitlines())
            hang = start(hang_state, hang_port, model='codex/hang')
            event(hang, 'ready')
            hang_token, hang_id = credential(hang_state)
            with concurrent.futures.ThreadPoolExecutor(max_workers=1) as executor:
                future = executor.submit(request, hang_port, hang_token, 'codex/hang')
                deadline = time.monotonic() + 5
                while inspect(hang_state, hang_id)['accepted'] == 0:
                    check(time.monotonic() < deadline, 'hanging request not admitted')
                    time.sleep(.03)
                while len(marker.read_text().splitlines()) <= previous_calls:
                    check(time.monotonic() < deadline, 'hanging fixture did not start')
                    time.sleep(.03)
                pid = int(marker.read_text().splitlines()[-1])
                check(stop(hang) == 0, 'signal cleanup failed')
                future.result(timeout=8)
                deadline = time.monotonic() + 5
                while True:
                    try:
                        os.kill(pid, 0)
                    except ProcessLookupError:
                        break
                    check(time.monotonic() < deadline, 'fixture process survived signal cleanup')
                    time.sleep(.03)
            resumed_hang = start(hang_state, hang_port, model='codex/hang')
            event(resumed_hang, 'ready')
            check(stop(resumed_hang) == 0, 'ownership was not released after signal')
            print('PASS: SIGINT cancels hanging fixture and releases ownership')

            revoked = run('keys', 'revoke', '--state-dir', str(state), identifier)
            check(revoked.returncode == 0, 'revoke failed')
            refused = run(*args(state, number))
            check(refused.returncode == 3, 'revoked setup was silently replaced')
            repair = run('init', 'repair', '--state-dir', str(state), '--discard', '--yes')
            check(repair.returncode == 0 and not (state / 'init' / 'client.key').exists() and not (state / 'init' / 'profile.json').exists(), 'repair failed')
            print('PASS: revoked setup requires explicit repair')

            malformed = run('init', '--provider', 'codex', '--json')
            check(malformed.returncode == 2, 'incomplete flags not rejected as usage')
            for process in processes:
                check(all(secret not in str(process.args) for secret in secrets), 'credential leaked in process arguments')
            for stream in streams:
                check(all(secret not in stream for secret in secrets), 'credential leaked in CLI output')
            for path in root.rglob('profile.json'):
                check(all(secret not in path.read_text() for secret in secrets), 'credential leaked in profile')
            print('PASS: malformed arguments and secret-free captured output/profile')
        finally:
            for proc in processes:
                if proc.poll() is None:
                    proc.send_signal(signal.SIGINT)
                    try:
                        proc.wait(timeout=5)
                    except subprocess.TimeoutExpired:
                        proc.kill()
                        proc.wait()


if __name__ == '__main__':
    main()
