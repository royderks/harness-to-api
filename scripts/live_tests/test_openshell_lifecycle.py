"""Offline lifecycle checks using an actual fake OpenShell executable."""
import contextlib
import importlib.util
import io
import os
from pathlib import Path
import subprocess
import tempfile
import time
import unittest
from unittest.mock import Mock, patch

ROOT = Path(__file__).resolve().parents[2]
spec = importlib.util.spec_from_file_location('openshell_lifecycle', ROOT / 'examples/openshell/main.py')
openshell = importlib.util.module_from_spec(spec)
spec.loader.exec_module(openshell)

FAKE = '''#!/usr/bin/env python3
import os
from pathlib import Path
import subprocess
import sys
import time
args = sys.argv[1:]
with open(os.environ['FAKE_LOG'], 'a') as log:
    log.write(' '.join(args[:2]) + "\\n")
if args[:2] == ['sandbox', 'delete'] and os.environ['FAKE_MODE'] == 'delete_timeout':
    Path(os.environ['FAKE_READY']).write_text('ready')
    time.sleep(30)
if args[:2] == ['sandbox', 'create']:
    mode = os.environ['FAKE_MODE']
    if mode == 'timeout':
        child = subprocess.Popen([sys.executable, '-c', "import time; from pathlib import Path; time.sleep(1.5); Path(" + repr(os.environ['FAKE_SURVIVED']) + ").write_text('alive')"])
        Path(os.environ['FAKE_PID']).write_text(str(child.pid))
        Path(os.environ['FAKE_READY']).write_text('ready')
        time.sleep(30)
    elif mode == 'status':
        print('sandbox created successfully')
    else:
        source = args[args.index('-c') + 1]
        exec(source.splitlines()[-1])
'''


class OpenShellLifecycle(unittest.TestCase):
    def invoke(self, mode):
        with tempfile.TemporaryDirectory() as temp:
            temp = Path(temp)
            executable = temp / 'openshell'
            executable.write_text(FAKE)
            executable.chmod(0o755)
            env = dict(os.environ, PATH=str(temp) + os.pathsep + os.environ.get('PATH', ''),
                       OPENAI_BASE_URL='https://example.invalid/v1', OPENAI_API_KEY='test-key',
                       FAKE_LOG=str(temp / 'log'), FAKE_MODE=mode,
                       FAKE_SURVIVED=str(temp / 'survived'), FAKE_PID=str(temp / 'pid'),
                       FAKE_READY=str(temp / 'ready'))
            popen = subprocess.Popen
            target = ['sandbox', 'create' if mode == 'timeout' else 'delete']
            def start(argv, **kwargs):
                process = popen(argv, **kwargs)
                if mode in ('timeout', 'delete_timeout') and argv[1:3] == target:
                    wait = process.wait
                    def wait_after_ready(timeout=None):
                        deadline = time.monotonic() + 15
                        while not (temp / 'ready').exists():
                            if time.monotonic() >= deadline:
                                process.kill()
                                wait()
                                self.fail('fake CLI did not reach the operation under test')
                            time.sleep(0.01)
                        return wait(timeout=0.1 if timeout is not None else None)
                    process.wait = wait_after_ready
                return process
            with patch.dict(os.environ, env, clear=True), \
                 patch.object(openshell.subprocess, 'Popen', side_effect=start), \
                 contextlib.redirect_stdout(io.StringIO()):
                if mode == 'success':
                    openshell.main()
                else:
                    with self.assertRaises((RuntimeError, subprocess.TimeoutExpired)):
                        openshell.main()
            calls = (temp / 'log').read_text().splitlines()
            self.assertEqual(calls[-3:], ['sandbox delete', 'provider delete', 'profile delete'])
            if mode == 'timeout':
                self.assertTrue((temp / 'pid').exists())
                time.sleep(1.6)
                self.assertFalse((temp / 'survived').exists(), 'descendant survived command timeout')

    def test_work_deadline_preserves_cleanup_budget(self):
        calls = []
        def command(args, timeout):
            calls.append((args[:2], timeout))
            if timeout <= 0:
                raise subprocess.TimeoutExpired(args, timeout)
            return b''
        clock = Mock()
        clock.monotonic.side_effect = [0, 1, 4, 8, 10]
        env = {'OPENAI_BASE_URL': 'https://example.invalid/v1', 'OPENAI_API_KEY': 'test-key'}
        with patch.dict(os.environ, env), patch.object(openshell, 'time', clock), \
             patch.object(openshell, 'WORK_SECONDS', 10), patch.object(openshell, 'cli', side_effect=command):
            with self.assertRaises(subprocess.TimeoutExpired):
                openshell.main()
        self.assertEqual([timeout for _, timeout in calls[:4]], [9, 6, 2, 0])
        self.assertEqual(calls[4:], [(('sandbox', 'delete'), 25),
                                    (('provider', 'delete'), 25), (('profile', 'delete'), 25)])

    @unittest.skipUnless(os.name == 'posix', 'POSIX process-group cleanup')
    def test_timeout_stops_descendant_and_cleans_resources(self):
        self.invoke('timeout')

    def test_status_only_is_not_a_completion(self):
        self.invoke('status')

    def test_cleanup_continues_after_delete_timeout(self):
        self.invoke('delete_timeout')

    def test_completion_marker_allows_success(self):
        self.invoke('success')


if __name__ == '__main__':
    unittest.main()
