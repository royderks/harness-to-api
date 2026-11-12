#!/usr/bin/env python3
"""Run the documented OpenShell route using task-owned names and a configured gateway."""
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import tempfile
import time
import urllib.parse
import uuid

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'examples'))
from _client import config

# Leave at least 110 seconds before the caller's 420-second timeout: 75 for
# three cleanup commands and the remainder for process termination and overhead.
WORK_SECONDS = 310
COMMAND_SECONDS = 180
CLEANUP_SECONDS = 25


def cli(args, timeout):
    if timeout <= 0:
        raise subprocess.TimeoutExpired(args, timeout)
    command = ['openshell', *args]
    # On POSIX, kill the entire command group, including descendants holding pipes.
    # Windows can only guarantee termination and reaping of the direct child.
    with tempfile.TemporaryFile() as out, tempfile.TemporaryFile() as err:
        process = subprocess.Popen(command, stdout=out, stderr=err,
                                   start_new_session=(os.name == 'posix'))
        try:
            process.wait(timeout=timeout)
        except subprocess.TimeoutExpired:
            if os.name == 'posix':
                try:
                    os.killpg(process.pid, signal.SIGKILL)
                except ProcessLookupError:
                    pass
            else:
                process.kill()
            process.wait()
            raise
        if os.name == 'posix':
            try:
                os.killpg(process.pid, signal.SIGKILL)
            except ProcessLookupError:
                pass
        out.seek(0)
        stdout = out.read()
        if process.returncode:
            err.seek(0)
            raise subprocess.CalledProcessError(process.returncode, command, stdout, err.read())
        return stdout



def main():
    deadline = time.monotonic() + WORK_SECONDS
    base, _, model = config(remote=True)
    url = urllib.parse.urlsplit(base)
    if not re.fullmatch(r'[A-Za-z0-9.-]+', url.hostname):
        raise ValueError('OpenShell profile requires a DNS hostname')
    name = 'h2a-live-' + uuid.uuid4().hex
    marker = 'H2A_COMPLETION_' + uuid.uuid4().hex
    with tempfile.TemporaryDirectory(prefix='h2a-live-openshell-') as temp:
        profile = Path(temp) / 'provider.yaml'
        source = (ROOT / 'examples/openshell/h2a-provider.yaml').read_text()
        source = source.replace('h2a-demo', name).replace('h2a.example.com', url.hostname)
        source = source.replace('port: 443', 'port: ' + str(url.port or 443))
        profile.write_text(source)
        profile.chmod(0o600)

        def work(*args):
            return cli(args, min(COMMAND_SECONDS, deadline - time.monotonic()))

        # Only delete successful creations or timed-out creations whose outcome
        # is unknown. Random full UUID names keep uncertain outcomes task-scoped.
        owned = []
        def create(resource, *args):
            try:
                work(*args)
            except subprocess.TimeoutExpired:
                owned.append(resource)
                raise
            owned.append(resource)

        cleanup_failed = False
        try:
            work('profile', 'lint', '-f', str(profile))
            create('profile', 'profile', 'import', '-f', str(profile))
            create('provider', 'provider', 'create', '--name', name, '--type', name,
                   '--credential', 'OPENAI_API_KEY')
            try:
                result = work('sandbox', 'create', '--name', name, '--from', 'python:3.13-slim',
                              '--provider', name, '--env', 'OPENAI_BASE_URL=' + base,
                              '--env', 'H2A_MODEL=' + model, '--no-tty', '--', 'python3', '-c',
                              (ROOT / 'examples/_client.py').read_text() + '\nprint(' + repr(marker) + ')\n')
            except (subprocess.TimeoutExpired, subprocess.CalledProcessError):
                # The sandbox command also runs the client, so nonzero can
                # mean the sandbox was created but the client failed.
                owned.append('sandbox')
                raise
            owned.append('sandbox')
            if marker.encode() not in result.splitlines():
                raise RuntimeError('Sandbox completion not confirmed')
        finally:
            for resource in reversed(owned):
                try:
                    cli((resource, 'delete', name), CLEANUP_SECONDS)
                except (OSError, subprocess.TimeoutExpired, subprocess.CalledProcessError):
                    cleanup_failed = True
        if cleanup_failed:
            raise RuntimeError('OpenShell cleanup not confirmed')
    print('OpenShell client completed and task resources removed.')


if __name__ == '__main__':
    try:
        main()
    except Exception:
        print('OpenShell live check failed; inspect task resources and gateway privately.', file=sys.stderr)
        sys.exit(1)
