#!/usr/bin/env python3
"""Run the documented OpenShell route using task-owned names and a configured gateway."""
from pathlib import Path
import re
import subprocess
import sys
import tempfile
import urllib.parse
import uuid

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / 'examples'))
from _client import config


def main():
    base, _, model = config(remote=True)
    url = urllib.parse.urlsplit(base)
    if not re.fullmatch(r'[A-Za-z0-9.-]+', url.hostname):
        raise ValueError('OpenShell profile requires a DNS hostname')
    name = 'h2a-live-' + uuid.uuid4().hex[:12]
    with tempfile.TemporaryDirectory(prefix='h2a-live-openshell-') as temp:
        profile = Path(temp) / 'provider.yaml'
        source = (ROOT / 'examples/openshell/h2a-provider.yaml').read_text()
        source = source.replace('h2a-demo', name).replace('h2a.example.com', url.hostname)
        source = source.replace('port: 443', 'port: ' + str(url.port or 443))
        profile.write_text(source)
        profile.chmod(0o600)

        def cli(*args):
            return subprocess.run(['openshell', *args], check=True, capture_output=True, timeout=180)

        cleanup_failed = False
        try:
            cli('profile', 'lint', '-f', str(profile))
            cli('profile', 'import', '-f', str(profile))
            cli('provider', 'create', '--name', name, '--type', name, '--credential', 'OPENAI_API_KEY')
            result = cli('sandbox', 'create', '--name', name, '--from', 'python:3.13-slim',
                         '--provider', name, '--env', 'OPENAI_BASE_URL=' + base,
                         '--env', 'H2A_MODEL=' + model, '--no-tty', '--', 'python3', '-c',
                         (ROOT / 'examples/_client.py').read_text())
            if not result.stdout.strip():
                raise RuntimeError('Empty sandbox output')
        finally:
            # Unique names scope cleanup even when a create response was lost.
            for resource in ('sandbox', 'provider', 'profile'):
                try:
                    result = subprocess.run(['openshell', resource, 'delete', name],
                                            capture_output=True, timeout=30)
                    cleanup_failed |= result.returncode != 0
                except (OSError, subprocess.TimeoutExpired):
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
