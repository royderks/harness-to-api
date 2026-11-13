#!/usr/bin/env python3
"""Local-only socket pressure regression. Requires a built h2a and Unix resource limits."""
import os
import signal
import socket
import subprocess
import tempfile
import time
from pathlib import Path

try:
    import resource
except ImportError:
    print('SKIP: resource limits unavailable')
    raise SystemExit(0)

binary = str(Path(os.environ.get('H2A_BIN', 'target/debug/h2a')).resolve(strict=True))
fixture = str(Path(os.environ.get('H2A_FIXTURE', 'target/debug/examples/harness-fixture')).resolve(strict=True))


def health(port, timeout=1):
    with socket.create_connection(('127.0.0.1', port), timeout) as conn:
        conn.settimeout(timeout)
        conn.sendall(b'GET /healthz HTTP/1.1\r\nHost: localhost\r\nConnection: close\r\n\r\n')
        return b'200 OK' in conn.recv(4096)


def run(limit, count, cap=False, cancel=False):
    with tempfile.TemporaryDirectory() as state:
        subprocess.run([binary, 'keys', 'create', '--state-dir', state, '--name', 'synthetic'],
                       check=True, stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        with socket.socket() as probe:
            probe.bind(('127.0.0.1', 0))
            port = probe.getsockname()[1]
        def set_limit():
            resource.setrlimit(resource.RLIMIT_NOFILE, (limit, limit))
        server = subprocess.Popen([binary, 'serve', '--state-dir', state, '--port', str(port),
                                   '--codex-bin', fixture],
                                  stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL,
                                  preexec_fn=set_limit)
        sockets = []
        try:
            for _ in range(100):
                try:
                    if health(port):
                        break
                except OSError:
                    time.sleep(.02)
            else:
                raise AssertionError('server did not start')
            for _ in range(count):
                sockets.append(socket.create_connection(('127.0.0.1', port), 1))
            time.sleep(.3)
            assert server.poll() is None, 'server exited during pressure'
            if cap:
                try:
                    assert not health(port, .2), 'health unexpectedly passed at capacity'
                except socket.timeout:
                    pass
            if cancel:
                started = time.monotonic()
                server.send_signal(signal.SIGINT)
                assert server.wait(timeout=2) == 0
                assert time.monotonic() - started < 2
            else:
                for conn in sockets:
                    conn.close()
                sockets.clear()
                for _ in range(30):
                    try:
                        if health(port, .3):
                            break
                    except OSError:
                        pass
                    time.sleep(.1)
                else:
                    raise AssertionError('health did not recover')
                assert server.poll() is None
        finally:
            for conn in sockets:
                conn.close()
            if server.poll() is None:
                server.send_signal(signal.SIGINT)
                server.wait(timeout=3)


run(512, 128, cap=True)
print('PASS: 128 idle connections block additional health; health recovers after release')
run(512, 128, cancel=True)
print('PASS: shutdown at connection capacity')
run(32, 48)
print('PASS: low descriptor limit survives pressure and recovers health')
run(32, 48, cancel=True)
print('PASS: shutdown under descriptor pressure')
